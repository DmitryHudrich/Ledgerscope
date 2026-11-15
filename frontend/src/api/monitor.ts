import { call, failure } from './client';
import type { BlockRangeSpan } from './types';

const API_BASE = import.meta.env.VITE_API_BASE ?? '/api';

export type CallOutcome = 'ok' | 'busy' | 'unsupported' | 'shallow' | 'failed' | 'gave_up';

export interface RpcCall {
  id: number;
  at_ms: number;
  chain_id: number;
  node: string;
  method: string;
  requests: number;
  attempt: number;
  latency_ms: number;
  outcome: CallOutcome;
  detail: string | null;
}

export interface NodeHealth {
  chain_id: number;
  host: string;
  in_pool: boolean;
  own: boolean;
  probe_latency_ms: number | null;
  shallow: boolean;
  saturated: boolean;
  unsupported: string[];
  calls: number;
  requests: number;
  failures: number;
  recent_error_rate: number;
  average_latency_ms: number | null;
  p50_latency_ms: number | null;
  p95_latency_ms: number | null;
  last_ok_ms: number | null;
  last_error: { at_ms: number; outcome: CallOutcome; detail: string } | null;
}

export interface ScanJob {
  id: number;
  chain_id: number;
  ranges: BlockRangeSpan[];
  total_blocks: number;
  done_blocks: number;
  percent: number;
  txs: number;
  started_ms: number;
  updated_ms: number;
  finished_ms: number | null;
  running: boolean;
  blocks_per_second: number | null;
  eta_ms: number | null;
  error: string | null;
}

export type BuildStage = 'coverage' | 'walk' | 'index' | 'classify' | 'actors' | 'tags';

export interface BuildJob {
  id: number;
  chain_id: number;
  stage: BuildStage;
  done: number;
  total: number;
  percent: number;
  txs: number;
  started_ms: number;
  updated_ms: number;
  finished_ms: number | null;
  running: boolean;
  error: string | null;
}

async function get<T>(path: string): Promise<T> {
  const response = await call(path, { method: 'GET' });
  if (!response.ok) throw await failure(response);
  return (await response.json()) as T;
}

const chainQuery = (chain: number | null, extra = '') => `?chain=${chain ?? 'all'}${extra}`;

export const monitorApi = {
  calls: (chain: number | null, limit = 500) => get<RpcCall[]>(`/monitor/calls${chainQuery(chain, `&limit=${limit}`)}`),
  nodes: (chain: number | null) => get<NodeHealth[]>(`/monitor/nodes${chainQuery(chain)}`),
  scans: (chain: number | null) => get<ScanJob[]>(`/monitor/scans${chainQuery(chain)}`),
};

export interface StreamHandlers {
  onCall: (call: RpcCall) => void;
  onScan: (job: ScanJob) => void;
  onBuild: (job: BuildJob) => void;
  onLagged: (missed: number) => void;
  onState: (connected: boolean) => void;
}

export function openMonitorStream(handlers: StreamHandlers): () => void {
  const source = new EventSource(`${API_BASE}/monitor/stream?chain=all`);
  source.onopen = () => handlers.onState(true);
  source.onerror = () => handlers.onState(false);
  source.addEventListener('call', (event) => handlers.onCall(JSON.parse((event as MessageEvent).data) as RpcCall));
  source.addEventListener('scan', (event) => handlers.onScan(JSON.parse((event as MessageEvent).data) as ScanJob));
  source.addEventListener('build', (event) => handlers.onBuild(JSON.parse((event as MessageEvent).data) as BuildJob));
  source.addEventListener('lagged', (event) => handlers.onLagged(Number((event as MessageEvent).data) || 0));
  return () => source.close();
}

export const OUTCOME_TITLE: Record<CallOutcome, string> = {
  ok: 'OK',
  busy: 'Busy',
  unsupported: 'Unsupported',
  shallow: 'No history',
  failed: 'Failed',
  gave_up: 'Gave up',
};

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ${seconds % 60} s`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}
