import type { GraphEdge } from './types';
import { parseWei } from '../lib/format';

export const ETH_ASSET = 'ETH';

export type EdgeFlavor = 'native' | 'token' | 'call' | 'deployment' | 'protocol';

export interface Endpoints {
  from: string;
  to: string;
}

export interface EdgeTransfer {
  key: string;
  symbol: string;
  decimals: number;
  native: boolean;
  amount: bigint;
}

export function edgeFlavor(edge: GraphEdge): EdgeFlavor {
  switch (edge.kind) {
    case 'native_transfer':
      return 'native';
    case 'contract_deployment':
      return 'deployment';
    case 'protocol':
      return 'protocol';
    case 'contract_interaction':
      return edge.action.kind === 'erc20_transfer' ? 'token' : 'call';
  }
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

export function edgeTransfer(edge: GraphEdge): EdgeTransfer | null {
  if (edge.kind === 'native_transfer') {
    return {
      key: ETH_ASSET,
      symbol: ETH_ASSET,
      decimals: 18,
      native: true,
      amount: parseWei(edge.amount),
    };
  }
  if (edge.kind === 'contract_interaction' && edge.action.kind === 'erc20_transfer') {
    return {
      key: edge.action.token.toLowerCase(),
      symbol: edge.action.token_name,
      decimals: edge.action.decimals,
      native: false,
      amount: parseWei(edge.action.amount),
    };
  }
  return null;
}

export function isTransferEdge(edge: GraphEdge): boolean {
  const flavor = edgeFlavor(edge);
  return flavor === 'native' || flavor === 'token';
}

export function edgeWei(edge: GraphEdge): bigint {
  if (!edge.succeeded || edge.kind !== 'native_transfer') return 0n;
  return parseWei(edge.amount);
}

export function contractAddressesOf(edge: GraphEdge): string[] {
  switch (edge.kind) {
    case 'contract_deployment':
      return [edge.contract_address];
    case 'contract_interaction':
      return edge.action.kind === 'erc20_transfer'
        ? [edge.contract_address, edge.action.token]
        : [edge.contract_address];
    default:
      return [];
  }
}

export function edgeLabel(edge: GraphEdge): string {
  switch (edge.kind) {
    case 'native_transfer':
      return 'ETH transfer';
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
