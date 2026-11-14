import { call, failure } from './client';

export interface IpWalletLink {
  src_ip: string;
  chain_id: number;
  address: string;
  hits: number;
  address_txs: number;
  score: number;
  confidence: number;
  first_seen: number;
  last_seen: number;
}

export interface CorrelateResult {
  from_block: number;
  to_block: number;
  matched_pairs: number;
  links_touched: number;
}

async function send<T>(path: string, method: string): Promise<T> {
  const response = await call(path, { method });
  if (!response.ok) throw await failure(response);
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

function query(chain?: number, extra?: Record<string, string>): string {
  const params = new URLSearchParams();
  if (chain !== undefined) params.set('chain', String(chain));
  for (const [key, value] of Object.entries(extra ?? {})) params.set(key, value);
  return params.size > 0 ? `?${params}` : '';
}

export interface CorrelateOptions {
  fromBlock?: number;
  upToBlock?: number;
  reset?: boolean;
}

export const tlsApi = {
  links: (chain?: number) => send<IpWalletLink[]>(`/tls/links${query(chain)}`, 'GET'),
  correlate: (chain?: number, options: CorrelateOptions = {}) => {
    const extra: Record<string, string> = {};
    if (options.fromBlock !== undefined) extra.from_block = String(options.fromBlock);
    if (options.upToBlock !== undefined) extra.up_to_block = String(options.upToBlock);
    if (options.reset) extra.reset = 'true';
    return send<CorrelateResult>(`/tls/correlate${query(chain, extra)}`, 'POST');
  },
  reset: (chain?: number) => send<void>(`/tls/links${query(chain)}`, 'DELETE'),
};
