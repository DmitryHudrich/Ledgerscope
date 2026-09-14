/** Wire format of `GET /graph` — mirrors `crates/web-api/src/dto.rs`. */

export interface TxResponse {
  tx_hash: string;
  block_number: number;
  timestamp: number;
  /** Wei, decimal string (u128 on the server side). */
  amount: string;
  from: string;
  to: string | null;
  /** Calldata, `0x`-prefixed hex. */
  data: string;
}

export interface GraphResponse {
  nodes: string[];
  edges: TxResponse[];
}

export interface GraphQuery {
  wallet: string;
  from: number;
  to: number;
}
