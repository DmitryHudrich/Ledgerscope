export interface TxMeta {
  tx_hash: string;
  block_number: number;
  timestamp: number;
  succeeded: boolean;
}

export type ContractAction =
  | {
      kind: 'erc20_transfer';
      token: string;
      from: string;
      to: string;

      amount: string;
      token_name: string;
      decimals: number;
    }
  | {
      kind: 'swap';
      protocol: string;
      family: 'uniswap_v2' | 'uniswap_v3' | 'uniswap_v4';
      pool: string;
      pool_id: string | null;
      fee: number | null;
      token_in: string;
      symbol_in: string;
      decimals_in: number;
      amount_in: string;
      token_out: string;
      symbol_out: string;
      decimals_out: number;
      amount_out: string;
      source: { kind: TagSourceKind; name: string; version: number };
    }
  | { kind: 'other' };

export type Interaction =
  | { kind: 'protocol' }
  | {
      kind: 'native_transfer';
      from: string;
      to: string;

      amount: string;
    }
  | {
      kind: 'asset_transfer';
      asset_id: string;
      symbol: string;
      decimals: number;
      from: string;
      to: string;

      amount: string;
    }
  | {
      kind: 'system_action';
      action: string;
      from: string;
      to: string | null;

      amount: string;
    }
  | { kind: 'contract_deployment'; deployer: string; contract_address: string }
  | {
      kind: 'contract_interaction';
      interactor: string;
      contract_address: string;
      action: ContractAction;
    };

export type GraphEdge = TxMeta & Interaction;

export interface BlockRangeSpan {
  from_block: number;
  to_block: number;
  block_count: number;
}

/** Read-only address identity attached by the backend label provider. */
export interface AddressLabel {
  value: string;
  source: string;
}

export interface GraphNode {
  address: string;
  labels: AddressLabel[];
  depth: number;
  root: boolean;
  expanded: boolean;
}

export interface LowLevelGraphActor {
  address: string;
  labels: AddressLabel[];
  depth: number;
  root: boolean;
  expanded: boolean;
}

export interface LowLevelInteraction {
  tx_hash: string;
  block_number: number;
  timestamp: number;
  succeeded: boolean;
  from: string;
  to: string;
  amount: string;
  deployment: boolean;
}

export interface LowLevelGraphResponse {
  actors: LowLevelGraphActor[];
  interactions: LowLevelInteraction[];
  span: BlockRangeSpan;
  filled_from_rpc: BlockRangeSpan[];
  truncated: boolean;
}

export type TagSourceKind = 'heuristic' | 'provider' | 'user' | 'agent';

export interface GraphTag {
  subject_kind: 'address' | 'tx';
  subject: string;
  key: string;
  category: string;
  confidence: number;
  source: { kind: TagSourceKind; name: string; version: number };
  evidence: string[];
  note: string;
}

export interface GraphResponse {
  nodes: GraphNode[];
  edges: GraphEdge[];
  tags?: GraphTag[];
  span: BlockRangeSpan;
  filled_from_rpc: BlockRangeSpan[];
  truncated: boolean;
  taint?: TaintInfo;
}

export interface TaintAsset {
  kind: 'native' | 'token' | 'issued';
  id: string | null;
}

export interface TaintFlow {
  tx_hash: string;
  block_number: number;
  timestamp: number;
  slot: number;
  kind: 'value' | 'transfer' | 'issued' | 'payout';
  asset: TaintAsset;
  from: string;
  to: string;
  amount: string;
  tainted: string;
  share_bps: number;
  source: { kind: TagSourceKind; name: string; version: number } | null;
}

export interface TaintHolding {
  address: string;
  asset: TaintAsset;
  total: string;
  tainted: string;
}

export interface TaintInfo {
  policy: string;
  min_share_bps: number;
  seeds: string[];
  sinks: string[];
  transit: string[];
  flows: TaintFlow[];
  holdings: TaintHolding[];
}

export interface GraphRoot {
  address: string;
  depth: number;
}

export type ChainFamily = 'evm' | 'tron';
export type ChainNetwork = 'mainnet' | 'testnet' | 'devnet';

export interface ChainInfo {
  chain_id: number;
  family: ChainFamily;
  network: ChainNetwork;
  name: string;
  slug: string;
  native_symbol: string;
  native_decimals: number;
  l2_of: number | null;
  rpc_nodes: number;
}

export interface ChainsResponse {
  default_chain: number;
  chains: ChainInfo[];
}

export type GraphScope = 'roots' | 'blocks' | 'taint';

export interface GraphQuery {
  chain?: number;
  scope?: GraphScope;
  roots: GraphRoot[];
  from_block: number;
  to_block: number;
  ranges?: Array<{ from_block: number; to_block: number; source?: 'auto' | 'rpc' | 'dataset'; exact?: boolean }>;
  confirm_rpc?: boolean;
  min_share_bps?: number;
}

export interface RpcConfirmation {
  status: 'rpc_confirmation_required';
  message: string;
  span: BlockRangeSpan;
  missing: BlockRangeSpan[];
  missing_blocks: number;
  indexed_blocks: number;
  dataset_blocks: number;
  exact_blocks: number;
}

export interface GraphLimits {
  max_roots: number;
  max_depth: number;
  max_nodes: number;
  max_edges: number;
  max_blocks: number;
}

export interface CoverageResponse {
  chain_id: number;
  persisted: boolean;
  chain_head: number | null;
  lowest_block: number | null;
  highest_block: number | null;
  block_count: number;
  tx_count: number;
  ranges: BlockRangeSpan[];
  approximate: BlockRangeSpan[];
  dataset: DatasetInfo | null;
  limits: GraphLimits;
}

export interface DatasetInfo {
  name: string;
  exact_values: boolean;
  available_until: number | null;
}

export interface HeadResponse {
  chain_id: number;
  head: number;
}

export interface HistogramBucket {
  from_block: number;
  to_block: number;
  indexed_blocks: number;
  tx_count: number;
}

export interface HistogramResponse {
  span: BlockRangeSpan | null;
  bucket_size: number;
  buckets: HistogramBucket[];
}

export interface HistogramQuery {
  chain?: number;
  from_block?: number;
  to_block?: number;
  buckets?: number;
}
