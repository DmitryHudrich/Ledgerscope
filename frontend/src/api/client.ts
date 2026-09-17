import type {
  CoverageResponse,
  GraphQuery,
  GraphResponse,
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

async function call(path: string, init: RequestInit, signal?: AbortSignal): Promise<Response> {
  try {
    return await fetch(`${API_BASE}${path}`, { ...init, signal });
  } catch (cause) {
    if (signal?.aborted) throw cause;
    throw new ApiError(UNREACHABLE);
  }
}

async function failure(response: Response): Promise<ApiError> {
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
  const response = await call(
    '/graph',
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        roots: query.roots,
        from_block: query.from_block,
        to_block: query.to_block,
        confirm_rpc: query.confirm_rpc ?? false,
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

export async function fetchCoverage(signal?: AbortSignal): Promise<CoverageResponse> {
  const response = await call('/coverage', { method: 'GET' }, signal);
  if (!response.ok) throw await failure(response);

  return (await response.json()) as CoverageResponse;
}

export async function fetchHistogram(
  query: HistogramQuery = {},
  signal?: AbortSignal,
): Promise<HistogramResponse> {
  const params = new URLSearchParams();
  if (query.from_block !== undefined) params.set('from_block', String(query.from_block));
  if (query.to_block !== undefined) params.set('to_block', String(query.to_block));
  if (query.buckets !== undefined) params.set('buckets', String(query.buckets));

  const suffix = params.size > 0 ? `?${params}` : '';
  const response = await call(`/coverage/histogram${suffix}`, { method: 'GET' }, signal);
  if (!response.ok) throw await failure(response);

  return (await response.json()) as HistogramResponse;
}
