import { useEffect, useMemo, useRef, useState } from 'react';

import type { ChainInfo } from '../../api/types';
import {
  OUTCOME_TITLE,
  formatDuration,
  monitorApi,
  type CallOutcome,
  type NodeHealth,
  type RpcCall,
} from '../../api/monitor';
import { formatCount, formatRelative } from '../../lib/format';
import type { MonitorState } from '../../lib/useMonitor';
import { IconClose, IconSearch } from '../Icons';
import { Badge, Button, Panel, cx, focusRing } from '../ui';

interface Props {
  monitor: MonitorState;
  chains: ChainInfo[];
  chainId: number | null;
  onClose: () => void;
}

type Tab = 'log' | 'nodes' | 'scans';
type Scope = 'current' | 'all';
type Show = 'all' | 'problems';

const SHOWN_ROWS = 400;

const outcomeTone: Record<CallOutcome, string> = {
  ok: 'text-good',
  busy: 'text-series-focus',
  unsupported: 'text-text-muted',
  shallow: 'text-text-muted',
  failed: 'text-critical',
  gave_up: 'text-critical',
};

function clock(ms: number): string {
  const at = new Date(ms);
  const pad = (value: number, size = 2) => String(value).padStart(size, '0');
  return `${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(at.getSeconds())}.${pad(at.getMilliseconds(), 3)}`;
}

function latencyTone(ms: number | null): string {
  if (ms === null) return 'text-text-muted';
  if (ms < 400) return 'text-good';
  if (ms < 1500) return 'text-text-primary';
  if (ms < 4000) return 'text-series-focus';
  return 'text-critical';
}

function segmented<T extends string>(value: T, options: Array<[T, string]>, onChange: (next: T) => void, label: string) {
  return (
    <div className="flex rounded-ui-sm border border-hairline bg-plane p-0.5" role="radiogroup" aria-label={label}>
      {options.map(([key, title]) => (
        <button
          key={key}
          type="button"
          role="radio"
          aria-checked={value === key}
          className={cx(
            'h-6 cursor-pointer rounded-[4px] px-2 text-[11px] font-medium',
            value === key ? 'bg-surface-2 text-text-primary shadow-[0_1px_2px_rgba(0,0,0,0.12)]' : 'text-text-muted hover:text-text-primary',
            focusRing,
          )}
          onClick={() => onChange(key)}
        >
          {title}
        </button>
      ))}
    </div>
  );
}

export function MonitorPanel({ monitor, chains, chainId, onClose }: Props) {
  const [tab, setTab] = useState<Tab>('log');
  const [scope, setScope] = useState<Scope>('current');
  const [show, setShow] = useState<Show>('all');
  const [query, setQuery] = useState('');
  const [follow, setFollow] = useState(true);
  const [nodes, setNodes] = useState<NodeHealth[] | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const list = useRef<HTMLDivElement>(null);

  const scoped = scope === 'current' ? chainId : null;
  const chainName = (id: number) => chains.find((chain) => chain.chain_id === id)?.name ?? `Chain ${id}`;

  useEffect(() => {
    if (tab !== 'nodes') return;
    let alive = true;
    const load = () => monitorApi.nodes(scoped).then((next) => alive && setNodes(next)).catch(() => undefined);
    void load();
    const timer = window.setInterval(load, 3000);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [scoped, tab]);

  const needle = query.trim().toLowerCase();
  const rows = useMemo(() => {
    const picked: RpcCall[] = [];
    for (let index = monitor.calls.length - 1; index >= 0 && picked.length < SHOWN_ROWS; index -= 1) {
      const call = monitor.calls[index];
      if (scoped !== null && call.chain_id !== scoped) continue;
      if (show === 'problems' && call.outcome === 'ok') continue;
      if (needle && !`${call.node} ${call.method} ${call.detail ?? ''}`.toLowerCase().includes(needle)) continue;
      picked.push(call);
    }
    return picked;
  }, [monitor.calls, needle, scoped, show]);

  useEffect(() => {
    if (follow && list.current) list.current.scrollTop = 0;
  }, [follow, rows]);

  const scans = monitor.scans.filter((job) => scoped === null || job.chain_id === scoped);
  const running = scans.filter((job) => job.running).length;

  return (
    <Panel
      className="absolute bottom-[calc(var(--sheet-h)+12px)] right-3 top-3 z-30 flex w-[min(640px,calc(100vw-80px))] flex-col overflow-hidden bg-surface-1 shadow-pop"
      role="dialog"
      aria-label="RPC monitor"
    >
      <header className="grid gap-2.5 border-b border-hairline px-4 pb-3 pt-3">
        <div className="flex items-center gap-2">
          <h2 className="m-0 text-[15px] font-semibold">RPC monitor</h2>
          <span className={cx('flex items-center gap-1.5 text-[11px]', monitor.connected ? 'text-good' : 'text-critical')}>
            <span className={cx('size-1.5 rounded-full', monitor.connected ? 'animate-pulse bg-good' : 'bg-critical')} />
            {monitor.connected ? 'live' : 'disconnected'}
          </span>
          <span className="text-[11px] tabular-nums text-text-muted">
            {monitor.recent.calls} calls / min{monitor.recent.problems > 0 ? ` · ${monitor.recent.problems} problems` : ''}
          </span>
          <Button variant="ghost" icon className="ml-auto" onClick={onClose} aria-label="Close the monitor">
            <IconClose />
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {segmented(tab, [['log', 'Live log'], ['nodes', 'Nodes'], ['scans', running > 0 ? `Scans · ${running}` : 'Scans']], setTab, 'Monitor view')}
          {segmented(scope, [['current', chainId !== null ? chainName(chainId) : 'Current'], ['all', 'All chains']], setScope, 'Chains')}
        </div>
        {tab === 'log' && (
          <div className="flex flex-wrap items-center gap-2">
            {segmented(show, [['all', 'Everything'], ['problems', 'Problems']], setShow, 'Outcomes')}
            <label className="flex h-7 min-w-[160px] flex-1 items-center gap-1.5 rounded-ui-sm border border-hairline bg-plane px-2 text-text-muted focus-within:border-accent">
              <IconSearch size={13} />
              <input
                className="h-full min-w-0 flex-1 bg-transparent text-xs text-text-primary outline-none placeholder:text-text-muted"
                placeholder="Node, method or error"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <Button className="h-7 px-2 text-xs" aria-pressed={monitor.paused} onClick={() => monitor.setPaused(!monitor.paused)}>
              {monitor.paused ? 'Resume' : 'Pause'}
            </Button>
            <Button variant="ghost" className="h-7 px-2 text-xs" onClick={monitor.clear}>Clear</Button>
          </div>
        )}
      </header>

      {tab === 'log' && (
        <div
          ref={list}
          className="min-h-0 flex-1 overflow-y-auto font-mono-ui text-[11px]"
          onScroll={(event) => setFollow(event.currentTarget.scrollTop < 8)}
        >
          {monitor.lagged > 0 && (
            <div className="border-b border-hairline px-4 py-1.5 font-ui text-[11px] text-series-focus">
              The log fell behind and skipped {formatCount(monitor.lagged)} calls.
            </div>
          )}
          {rows.length === 0 && (
            <p className="m-0 px-4 py-10 text-center font-ui text-xs text-text-muted">
              {monitor.calls.length === 0 ? 'No RPC calls yet. Build a graph over blocks that are not stored.' : 'Nothing matches the filters.'}
            </p>
          )}
          {rows.map((call) => (
            <div key={call.id} className="border-b border-[color-mix(in_srgb,var(--hairline)_60%,transparent)]">
              <button
                type="button"
                className={cx(
                  'grid w-full cursor-pointer grid-cols-[84px_minmax(0,1fr)_minmax(0,1fr)_56px_76px] items-center gap-2 px-4 py-1 text-left hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]',
                  call.outcome !== 'ok' && 'bg-[color-mix(in_srgb,var(--critical)_4%,transparent)]',
                )}
                onClick={() => setExpanded(expanded === call.id ? null : call.id)}
                title={call.detail ?? undefined}
              >
                <span className="tabular-nums text-text-muted">{clock(call.at_ms)}</span>
                <span className="truncate text-text-secondary">{call.node}</span>
                <span className="truncate">
                  {call.method}
                  {call.requests > 1 && <span className="text-text-muted"> ×{call.requests}</span>}
                  {call.attempt > 1 && <span className="text-series-focus"> try {call.attempt}</span>}
                </span>
                <span className={cx('text-right tabular-nums', latencyTone(call.latency_ms))}>{call.latency_ms} ms</span>
                <span className={cx('text-right font-ui font-medium', outcomeTone[call.outcome])}>{OUTCOME_TITLE[call.outcome]}</span>
              </button>
              {expanded === call.id && (
                <div className="grid gap-0.5 bg-plane px-4 py-2 font-ui text-[11px] text-text-secondary">
                  <span>{chainName(call.chain_id)} · attempt {call.attempt} · {call.requests} {call.requests === 1 ? 'request' : 'requests in a batch'}</span>
                  {call.detail && <span className="break-words font-mono-ui text-critical">{call.detail}</span>}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {tab === 'nodes' && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {nodes === null && <p className="m-0 px-4 py-6 text-xs text-text-muted">Loading…</p>}
          {nodes?.length === 0 && <p className="m-0 px-4 py-6 text-xs text-text-muted">No nodes yet.</p>}
          {nodes?.map((node) => {
            const errorRate = Math.round(node.recent_error_rate * 100);
            const state = !node.in_pool ? 'Left the pool' : errorRate >= 30 ? 'Failing' : errorRate > 0 || node.saturated ? 'Strained' : node.calls === 0 ? 'Idle' : 'Healthy';
            const tone = state === 'Failing' ? 'bg-critical' : state === 'Strained' ? 'bg-series-focus' : state === 'Healthy' ? 'bg-good' : 'bg-text-muted opacity-50';
            return (
              <div key={`${node.chain_id}:${node.host}`} className="grid gap-1.5 border-b border-hairline px-4 py-2.5">
                <div className="flex items-center gap-2">
                  <span className={cx('size-2 flex-none rounded-full', tone)} title={state} />
                  <span className="min-w-0 truncate font-mono-ui text-xs font-medium">{node.host}</span>
                  {node.own && <Badge>own</Badge>}
                  {scope === 'all' && <span className="text-[11px] text-text-muted">{chainName(node.chain_id)}</span>}
                  <span className="ml-auto text-[11px] text-text-muted">{state}</span>
                </div>
                <div className="grid grid-cols-4 gap-2 text-[11px]">
                  <div><div className="text-text-muted">Calls</div><div className="tabular-nums">{formatCount(node.calls)}</div></div>
                  <div>
                    <div className="text-text-muted">Errors (last 200)</div>
                    <div className="flex items-center gap-1.5">
                      <span className="tabular-nums">{errorRate}%</span>
                      <span className="h-1 flex-1 overflow-hidden rounded-full bg-hairline">
                        <span className={cx('block h-full', errorRate >= 30 ? 'bg-critical' : 'bg-series-focus')} style={{ width: `${errorRate}%` }} />
                      </span>
                    </div>
                  </div>
                  <div><div className="text-text-muted">p50 / p95</div><div className={cx('tabular-nums', latencyTone(node.p50_latency_ms))}>{node.p50_latency_ms ?? '—'} / {node.p95_latency_ms ?? '—'} ms</div></div>
                  <div><div className="text-text-muted">Last answer</div><div className="tabular-nums">{node.last_ok_ms ? formatRelative(node.last_ok_ms / 1000) : '—'}</div></div>
                </div>
                {(node.shallow || node.saturated || node.unsupported.length > 0 || node.probe_latency_ms !== null) && (
                  <div className="flex flex-wrap gap-1.5 text-[10px] text-text-muted">
                    {node.probe_latency_ms !== null && <span>probe {node.probe_latency_ms} ms</span>}
                    {node.saturated && <span className="text-series-focus">rate limit reached</span>}
                    {node.shallow && <span>short history</span>}
                    {node.unsupported.length > 0 && <span>no {node.unsupported.join(', ')}</span>}
                  </div>
                )}
                {node.last_error && (
                  <div className="truncate text-[11px] text-critical" title={node.last_error.detail}>
                    {OUTCOME_TITLE[node.last_error.outcome]} {formatRelative(node.last_error.at_ms / 1000)}: {node.last_error.detail}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {tab === 'scans' && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {scans.length === 0 && <p className="m-0 px-4 py-10 text-center text-xs text-text-muted">No block scans yet.</p>}
          {scans.map((job) => {
            const percent = Math.min(100, job.percent);
            return (
              <div key={job.id} className="grid gap-1.5 border-b border-hairline px-4 py-3">
                <div className="flex items-center gap-2 text-xs">
                  <span className="font-medium">{chainName(job.chain_id)}</span>
                  <span className="text-text-muted">
                    {job.ranges.length === 1
                      ? `${formatCount(job.ranges[0].from_block)} – ${formatCount(job.ranges[0].to_block)}`
                      : `${job.ranges.length} gaps`}
                  </span>
                  <span className={cx('ml-auto text-[11px] font-medium', job.error ? 'text-critical' : job.running ? 'text-accent' : 'text-good')}>
                    {job.error ? 'Failed' : job.running ? 'Running' : 'Done'}
                  </span>
                </div>
                <div className="h-1.5 overflow-hidden rounded-full bg-hairline">
                  <div className={cx('h-full rounded-full', job.error ? 'bg-critical' : job.running ? 'bg-accent' : 'bg-good')} style={{ width: `${percent}%` }} />
                </div>
                <div className="flex flex-wrap gap-x-3 text-[11px] tabular-nums text-text-secondary">
                  <span>{formatCount(job.done_blocks)} / {formatCount(job.total_blocks)} blocks · {percent.toFixed(0)}%</span>
                  {job.blocks_per_second !== null && <span>{job.blocks_per_second.toFixed(1)} blocks/s</span>}
                  {job.running && job.eta_ms !== null && <span>~{formatDuration(job.eta_ms)} left</span>}
                  <span>{formatCount(job.txs)} txs</span>
                  <span>{formatDuration((job.finished_ms ?? job.updated_ms) - job.started_ms)}</span>
                  {!job.running && <span className="text-text-muted">{formatRelative((job.finished_ms ?? job.updated_ms) / 1000)}</span>}
                </div>
                {job.error && <div className="break-words text-[11px] text-critical">{job.error}</div>}
              </div>
            );
          })}
        </div>
      )}
    </Panel>
  );
}
