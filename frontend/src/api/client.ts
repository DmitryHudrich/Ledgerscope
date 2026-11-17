import type {
  ChainsResponse,
  CoverageResponse,
  GraphQuery,
  GraphResponse,
  HeadResponse,
  HistogramQuery,
  HistogramResponse,
  RpcConfirmation,
} from './types';

const API_BASE = import.meta.env.VITE_API_BASE ?? '/api';

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status?: number,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export class RpcConfirmationRequired extends Error {
  constructor(readonly confirmation: RpcConfirmation) {
    super(confirmation.message);
    this.name = 'RpcConfirmationRequired';
  }
}

const UNREACHABLE = 'Backend unreachable. Start `cargo run -p ledgerscope-web-api`.';

export async function call(path: string, init: RequestInit, signal?: AbortSignal): Promise<Response> {
  try {
    return await fetch(`${API_BASE}${path}`, { ...init, signal });
  } catch (cause) {
    if (signal?.aborted) throw cause;
    throw new ApiError(UNREACHABLE);
  }
}

export async function failure(response: Response): Promise<ApiError> {
  const body = await response.text().catch(() => '');
  let message = body.trim();

  try {
    const parsed = JSON.parse(body) as { error?: string };
    if (parsed?.error) message = parsed.error;
  } catch {
    /* the body was not json, keep it as it came */
  }

  return new ApiError(
    message || `Request failed with ${response.status} ${response.statusText}`,
    response.status,
  );
}

export async function fetchGraph(
  query: GraphQuery,
  signal?: AbortSignal,
): Promise<GraphResponse> {
  const taint = query.scope === 'taint';
  const response = await call(
    taint ? '/graph/taint' : '/graph',
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        chain: query.chain,
        scope: taint ? 'roots' : (query.scope ?? 'roots'),
        roots: query.scope === 'blocks' ? [] : query.roots,
        from_block: query.from_block,
        to_block: query.to_block,
        ranges: query.ranges ?? [],
        confirm_rpc: query.confirm_rpc ?? false,
        ...(taint && query.min_share_bps !== undefined ? { min_share_bps: query.min_share_bps } : {}),
      }),
    },
    signal,
  );

  if (response.status === 409) {
    const confirmation = (await response.json()) as RpcConfirmation;
    throw new RpcConfirmationRequired(confirmation);
  }

  if (!response.ok) throw await failure(response);

  const payload = (await response.json()) as GraphResponse;
  if (!payload || !Array.isArray(payload.nodes) || !Array.isArray(payload.edges)) {
    throw new ApiError('Malformed response: expected { nodes, edges }.');
  }
  return payload;
}

export async function fetchChains(signal?: AbortSignal): Promise<ChainsResponse> {
  const response = await call('/chains', { method: 'GET' }, signal);
  if (!response.ok) throw await failure(response);

  const payload = (await response.json()) as ChainsResponse;
  if (!payload || !Array.isArray(payload.chains)) {
    throw new ApiError('Malformed response: expected { chains }.');
  }
  return payload;
}

export async function fetchCoverage(chain?: number, signal?: AbortSignal): Promise<CoverageResponse> {
  const suffix = chain === undefined ? '' : `?chain=${chain}`;
  const response = await call(`/coverage${suffix}`, { method: 'GET' }, signal);
  if (!response.ok) throw await failure(response);

  return (await response.json()) as CoverageResponse;
}

export async function fetchHead(chain?: number): Promise<HeadResponse> {
  const suffix = chain === undefined ? '' : `?chain=${chain}`;
  const response = await call(`/head${suffix}`, { method: 'GET' });
  if (!response.ok) throw await failure(response);

  return (await response.json()) as HeadResponse;
}

export async function fetchHistogram(
  query: HistogramQuery = {},
  signal?: AbortSignal,
): Promise<HistogramResponse> {
  const params = new URLSearchParams();
  if (query.chain !== undefined) params.set('chain', String(query.chain));
  if (query.from_block !== undefined) params.set('from_block', String(query.from_block));
  if (query.to_block !== undefined) params.set('to_block', String(query.to_block));
  if (query.buckets !== undefined) params.set('buckets', String(query.buckets));

  const suffix = params.size > 0 ? `?${params}` : '';
  const response = await call(`/coverage/histogram${suffix}`, { method: 'GET' }, signal);
  if (!response.ok) throw await failure(response);

  return (await response.json()) as HistogramResponse;
}
