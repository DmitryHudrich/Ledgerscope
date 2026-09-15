export interface TxMeta {
  tx_hash: string;
  block_number: number;
  timestamp: number;
}

export type ContractAction =
  | {
      kind: 'erc20_transfer';
      from: string;
      to: string;

      amount: string;
      token_name: string;
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
  | { kind: 'contract_deployment'; deployer: string; contract_address: string }
  | {
      kind: 'contract_interaction';
      interactor: string;
      contract_address: string;
      action: ContractAction;
    };

export type GraphEdge = TxMeta & Interaction;

export interface GraphResponse {
  nodes: string[];
  edges: GraphEdge[];
}

export interface GraphQuery {
  wallet: string;
  from: number;
  to: number;
}
