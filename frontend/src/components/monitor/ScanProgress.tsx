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

  return (
    <div className="grid w-full min-w-0 gap-2 text-left">
      <div className="flex items-baseline justify-between gap-3 text-xs">
        <span className="font-medium tabular-nums text-text-primary">
          {formatCount(job.done_blocks)} / {formatCount(job.total_blocks)} blocks
        </span>
        <span className="text-[15px] font-semibold tabular-nums">{percent.toFixed(percent < 10 ? 1 : 0)}%</span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-hairline" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(percent)}>
        <div className="h-full rounded-full bg-accent [transition:width_300ms_ease]" style={{ width: `${percent}%` }} />
      </div>
      <div className="flex flex-wrap justify-between gap-x-3 gap-y-0.5 text-[11px] text-text-secondary">
        <span className="tabular-nums">
          {job.blocks_per_second ? `${job.blocks_per_second.toFixed(job.blocks_per_second < 10 ? 1 : 0)} blocks/s` : 'measuring speed…'}
        </span>
        <span className="tabular-nums">
          {job.eta_ms !== null ? `~${formatDuration(job.eta_ms)} left` : '—'}
        </span>
        <span className="tabular-nums">{formatCount(job.txs)} txs</span>
        <span className="tabular-nums">elapsed {formatDuration((job.running ? Date.now() : job.updated_ms) - job.started_ms)}</span>
      </div>
      <button
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
      </button>
    </div>
  );
}
