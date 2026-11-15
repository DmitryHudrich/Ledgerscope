import { call, failure } from './client';

export interface TableUsage {
  table: string;
  rows: number;
  bytes_on_disk: number;
  uncompressed_bytes: number;
}

export interface ChainStorage {
  chain_id: number;
  name: string | null;
  txs: number;
  estimated_bytes: number;
}

export interface StorageUsage {
  bytes_on_disk: number;
  uncompressed_bytes: number;
  tables: TableUsage[];
  chains: ChainStorage[];
}

export interface ChainTraffic {
  chain_id: number;
  name: string;
  calls: number;
  sent_bytes: number;
  received_bytes: number;
}

export interface Usage {
  since: string;
  storage: StorageUsage | null;
  traffic: {
    calls: number;
    sent_bytes: number;
    received_bytes: number;
    chains: ChainTraffic[];
  };
}

export async function fetchUsage(signal?: AbortSignal): Promise<Usage> {
  const response = await call('/info/usage', {}, signal);
  if (!response.ok) throw await failure(response);
  return (await response.json()) as Usage;
}
