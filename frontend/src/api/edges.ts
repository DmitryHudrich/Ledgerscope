import type { GraphEdge } from './types';
import { formatAmount, parseWei } from '../lib/format';
import { nativeDecimals, nativeSymbol } from '../lib/chain';

export const ETH_ASSET = 'ETH';

export type EdgeFlavor = 'native' | 'token' | 'swap' | 'call' | 'deployment' | 'protocol';

const NATIVE_ADDRESS = '0x0000000000000000000000000000000000000000';

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
    case 'asset_transfer':
      return 'token';
    case 'system_action':
      return 'call';
    case 'contract_deployment':
      return 'deployment';
    case 'protocol':
      return 'protocol';
    case 'contract_interaction':
      if (edge.action.kind === 'swap') return 'swap';
      return edge.action.kind === 'erc20_transfer' ? 'token' : 'call';
  }
}

export function edgeEndpoints(edge: GraphEdge): Endpoints | null {
  switch (edge.kind) {
    case 'native_transfer':
    case 'asset_transfer':
      return { from: edge.from, to: edge.to };
    case 'system_action':
      return edge.to ? { from: edge.from, to: edge.to } : null;
    case 'contract_deployment':
      return { from: edge.deployer, to: edge.contract_address };
    case 'contract_interaction':
      if (edge.action.kind === 'swap') return { from: edge.interactor, to: edge.action.pool };
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
      symbol: nativeSymbol(),
      decimals: nativeDecimals(),
      native: true,
      amount: parseWei(edge.amount),
    };
  }
  if (edge.kind === 'asset_transfer') {
    return {
      key: `asset:${edge.asset_id}`,
      symbol: edge.symbol,
      decimals: edge.decimals,
      native: false,
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

export interface SwapSide {
  key: string;
  symbol: string;
  decimals: number;
  native: boolean;
  amount: bigint;
}

export interface EdgeSwap {
  protocol: string;
  fee: string | null;
  pool: string;
  poolId: string | null;
  router: string;
  tokenIn: SwapSide;
  tokenOut: SwapSide;
}

function swapSide(token: string, symbol: string, decimals: number, amount: string): SwapSide {
  const native = token.toLowerCase() === NATIVE_ADDRESS;
  return {
    key: native ? ETH_ASSET : token.toLowerCase(),
    symbol: native ? nativeSymbol() : symbol,
    decimals: native ? nativeDecimals() : decimals,
    native,
    amount: parseWei(amount),
  };
}

function feeLabel(fee: number | null): string | null {
  return fee === null ? null : `${fee / 10000}%`;
}

export function edgeSwap(edge: GraphEdge): EdgeSwap | null {
  if (edge.kind !== 'contract_interaction' || edge.action.kind !== 'swap') return null;
  const action = edge.action;
  return {
    protocol: action.protocol,
    fee: feeLabel(action.fee),
    pool: action.pool,
    poolId: action.pool_id,
    router: edge.contract_address,
    tokenIn: swapSide(action.token_in, action.symbol_in, action.decimals_in, action.amount_in),
    tokenOut: swapSide(action.token_out, action.symbol_out, action.decimals_out, action.amount_out),
  };
}

export function swapAmounts(swap: EdgeSwap): string {
  const { tokenIn, tokenOut } = swap;
  return `${formatAmount(tokenIn.amount, tokenIn.decimals, tokenIn.symbol)} → ${formatAmount(tokenOut.amount, tokenOut.decimals, tokenOut.symbol)}`;
}

export function swapSummary(swap: EdgeSwap): string {
  return [swapAmounts(swap), swap.protocol, swap.fee].filter((part) => part !== null).join(' · ');
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
      if (edge.action.kind === 'swap') {
        return [
          edge.contract_address,
          edge.action.pool,
          edge.action.token_in,
          edge.action.token_out,
        ].filter((address) => address.toLowerCase() !== NATIVE_ADDRESS);
      }
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
      return `${nativeSymbol()} transfer`;
    case 'asset_transfer':
      return `${edge.symbol} transfer`;
    case 'system_action':
      return humanizeAction(edge.action);
    case 'contract_deployment':
      return 'deployment';
    case 'protocol':
      return 'protocol';
    case 'contract_interaction':
      if (edge.action.kind === 'swap') return `${edge.action.protocol} swap`;
      return edge.action.kind === 'erc20_transfer'
        ? `${edge.action.token_name} transfer`
        : 'contract call';
  }
}

function humanizeAction(action: string): string {
  const words = action.replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}
