import type { GraphQuery, GraphResponse } from './types';

/**
 * Vite proxies `/api` to the web-api binary (see vite.config.ts). Override with
 * `VITE_API_BASE` when serving the built bundle from somewhere else.
 */
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

export async function fetchGraph(
  query: GraphQuery,
  signal?: AbortSignal,
): Promise<GraphResponse> {
  const params = new URLSearchParams({
    wallet: query.wallet,
    from: String(query.from),
    to: String(query.to),
  });

  let response: Response;
  try {
    response = await fetch(`${API_BASE}/graph?${params}`, { signal });
  } catch (cause) {
    if (signal?.aborted) throw cause;
    throw new ApiError(
      'Backend unreachable. Start `cargo run -p ledgerscope-web-api` or switch to demo data.',
    );
  }

  if (!response.ok) {
    const body = await response.text().catch(() => '');
    throw new ApiError(
      body.trim() || `Request failed with ${response.status} ${response.statusText}`,
      response.status,
    );
  }

  const payload = (await response.json()) as GraphResponse;
  if (!payload || !Array.isArray(payload.nodes) || !Array.isArray(payload.edges)) {
    throw new ApiError('Malformed response: expected { nodes, edges }.');
  }
  return payload;
}
