import type { RpcCall, ScanJob } from '../../api/monitor';
import { formatDuration } from '../../api/monitor';
import { formatCount } from '../../lib/format';
import { cx } from '../ui';

interface Props {
  job: ScanJob | null;
  calls: RpcCall[];
  onOpenMonitor: () => void;
}

const RECENT_MS = 15_000;

export function ScanProgress({ job, calls, onOpenMonitor }: Props) {
  if (!job) return null;

  const since = Date.now() - RECENT_MS;
  const recent = calls.filter((call) => call.chain_id === job.chain_id && call.at_ms >= since);
  const problems = recent.filter((call) => call.outcome !== 'ok').length;
  const nodes = new Set(recent.map((call) => call.node)).size;
  const last = recent[recent.length - 1];
  const percent = Math.min(100, Math.max(0, job.percent));
  const warming = job.running && job.done_blocks === 0 && !job.transfer;
  const dataset = job.source === 'dataset';
  const transfer = job.transfer;
  const transferred = transfer && transfer.total_bytes > 0
    ? Math.min(100, (transfer.done_bytes / transfer.total_bytes) * 100)
    : 0;
  const elapsed = (job.running ? Date.now() : job.updated_ms) - job.started_ms;

  return (
    <div className="grid w-full min-w-0 gap-2 text-left">
      {job.source && (
        <span
          className={cx(
            'justify-self-start rounded-full px-2 py-px text-[10px] font-semibold uppercase tracking-wide',
            dataset ? 'bg-[color-mix(in_srgb,var(--series-token)_14%,transparent)] text-series-token' : 'bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] text-accent',
          )}
        >
          {dataset ? 'AWS public dataset' : 'JSON-RPC'}
        </span>
      )}
      <div className="flex items-baseline justify-between gap-3 text-xs">
        <span className="font-medium tabular-nums text-text-primary">
          {formatCount(job.done_blocks)} / {formatCount(job.total_blocks)} blocks
        </span>
        <span className="text-[15px] font-semibold tabular-nums">{warming ? '…' : `${percent.toFixed(percent < 10 ? 1 : 0)}%`}</span>
      </div>
      <div
        className="relative h-2 overflow-hidden rounded-full bg-hairline"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={warming ? undefined : Math.round(percent)}
        aria-busy={warming}
      >
        {warming ? (
          <div className="absolute inset-y-0 w-1/3 animate-[scan-sweep_1.4s_ease-in-out_infinite] rounded-full bg-accent opacity-70" />
        ) : (
          <div className="h-full rounded-full bg-accent [transition:width_300ms_ease]" style={{ width: `${percent}%` }} />
        )}
      </div>
      {transfer && (
        <div className="grid gap-1 rounded-ui-sm border border-hairline bg-plane px-2 py-1.5">
          <div className="flex items-baseline justify-between gap-2 text-[11px]">
            <span className="min-w-0 truncate text-text-secondary">Downloading {transfer.label}</span>
            <span className="flex-none tabular-nums text-text-primary">
              {megabytes(transfer.done_bytes)} / {megabytes(transfer.total_bytes)} MB
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-hairline" role="progressbar" aria-label="Download" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(transferred)}>
            <div className="h-full rounded-full bg-series-token [transition:width_300ms_ease]" style={{ width: `${transferred}%` }} />
          </div>
        </div>
      )}
      {warming && (
        <p className="m-0 text-[11px] leading-snug text-text-muted">
          {dataset
            ? 'Reading the dataset files of this day. The bar moves once the first window is stored.'
            : 'Waiting for the first window of blocks.'}
        </p>
      )}
      <div className="flex flex-wrap justify-between gap-x-3 gap-y-0.5 text-[11px] text-text-secondary">
        <span className="tabular-nums">
          {job.blocks_per_second ? `${job.blocks_per_second.toFixed(job.blocks_per_second < 10 ? 1 : 0)} blocks/s` : 'measuring speed…'}
        </span>
        <span className="tabular-nums">
          {job.eta_ms !== null ? `~${formatDuration(job.eta_ms)} left` : '—'}
        </span>
        <span className="tabular-nums">{formatCount(job.txs)} txs</span>
        <span className="tabular-nums">elapsed {formatDuration(elapsed)}</span>
      </div>
      {!dataset && <button
        type="button"
        className="flex min-w-0 cursor-pointer items-center gap-2 overflow-hidden rounded-ui-sm border border-hairline bg-plane px-2 py-1.5 text-[11px] text-text-secondary hover:border-hairline-strong"
        onClick={onOpenMonitor}
        title="Open the RPC monitor"
      >
        <span className={cx('size-1.5 flex-none rounded-full', problems === 0 ? 'bg-good' : problems * 3 > recent.length ? 'bg-critical' : 'bg-series-focus')} />
        <span className="flex-none whitespace-nowrap tabular-nums">{recent.length} calls on {nodes} {nodes === 1 ? 'node' : 'nodes'} in 15 s{problems > 0 ? ` · ${problems} retried` : ''}</span>
        {last && (
          <span className="ml-auto min-w-0 truncate font-mono-ui text-text-muted">
            {last.node} · {last.method} · {last.latency_ms} ms
          </span>
        )}
      </button>}
    </div>
  );
}

function megabytes(bytes: number): string {
  return (bytes / 1_000_000).toFixed(bytes < 10_000_000 ? 1 : 0);
}
