import type { GraphEdge } from './types';
import { parseWei } from '../lib/format';

export interface Endpoints {
  from: string;
  to: string;
}

export function edgeEndpoints(edge: GraphEdge): Endpoints | null {
  switch (edge.kind) {
    case 'native_transfer':
      return { from: edge.from, to: edge.to };
    case 'contract_deployment':
      return { from: edge.deployer, to: edge.contract_address };
    case 'contract_interaction':

      return edge.action.kind === 'erc20_transfer'
        ? { from: edge.action.from, to: edge.action.to }
        : { from: edge.interactor, to: edge.contract_address };
    case 'protocol':
      return null;
  }
}

export function edgeWei(edge: GraphEdge): bigint {
  return edge.kind === 'native_transfer' ? parseWei(edge.amount) : 0n;
}

export function contractAddressOf(edge: GraphEdge): string | null {
  switch (edge.kind) {
    case 'contract_deployment':
    case 'contract_interaction':
      return edge.contract_address;
    default:
      return null;
  }
}

export function isContractEdge(edge: GraphEdge): boolean {
  return edge.kind !== 'native_transfer';
}

export function edgeLabel(edge: GraphEdge): string {
  switch (edge.kind) {
    case 'native_transfer':
      return 'transfer';
    case 'contract_deployment':
      return 'deployment';
    case 'protocol':
      return 'protocol';
    case 'contract_interaction':
      return edge.action.kind === 'erc20_transfer'
        ? `${edge.action.token_name} transfer`
        : 'contract call';
  }
}
