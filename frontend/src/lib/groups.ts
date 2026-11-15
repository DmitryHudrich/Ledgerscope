import { edgeEndpoints, edgeTransfer } from '../api/edges';
import type { GraphEdge, GraphResponse } from '../api/types';

export interface GraphGroup {
  id: string;
  name: string;
  color: string;
  nodes: string[];
  txs: string[];
  hidden: boolean;
}

export const GROUP_COLORS = ['#e8590c', '#2f9e44', '#7048e8', '#1c7ed6', '#d6336c', '#f08c00', '#0c8599', '#5c940d'];

export function nextGroupColor(groups: GraphGroup[]): string {
  const used = new Set(groups.map((group) => group.color));
  return GROUP_COLORS.find((color) => !used.has(color)) ?? GROUP_COLORS[groups.length % GROUP_COLORS.length];
}

export function newGroupId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function hiddenSets(
  hiddenNodes: Set<string>,
  hiddenTxs: Set<string>,
  groups: GraphGroup[],
): { nodes: Set<string>; txs: Set<string> } {
  const nodes = new Set(hiddenNodes);
  const txs = new Set(hiddenTxs);
  for (const group of groups) {
    if (!group.hidden) continue;
    for (const id of group.nodes) nodes.add(id);
    for (const hash of group.txs) txs.add(hash);
  }
  return { nodes, txs };
}

export function withoutTxs(response: GraphResponse, txs: Set<string>): GraphResponse {
  if (txs.size === 0) return response;
  return { ...response, edges: response.edges.filter((edge) => !txs.has(edge.tx_hash.toLowerCase())) };
}

export interface AssetFlowStats {
  key: string;
  symbol: string;
  decimals: number;
  inflow: bigint;
  outflow: bigint;
  internal: bigint;
  volume: bigint;
}

export interface GroupStats {
  nodes: number;
  txs: number;
  failed: number;
  counterparties: number;
  firstBlock: number | null;
  lastBlock: number | null;
  firstTime: number | null;
  lastTime: number | null;
  flows: AssetFlowStats[];
}

export function groupStats(group: GraphGroup, edges: GraphEdge[]): GroupStats {
  const members = new Set(group.nodes);
  const chosen = new Set(group.txs);
  const txs = new Set<string>();
  const failed = new Set<string>();
  const outside = new Set<string>();
  const flows = new Map<string, AssetFlowStats>();
  let firstBlock: number | null = null;
  let lastBlock: number | null = null;
  let firstTime: number | null = null;
  let lastTime: number | null = null;

  for (const edge of edges) {
    const hash = edge.tx_hash.toLowerCase();
    const ends = edgeEndpoints(edge);
    const from = ends?.from.toLowerCase() ?? null;
    const to = ends?.to.toLowerCase() ?? null;
    const fromIn = from !== null && members.has(from);
    const toIn = to !== null && members.has(to);
    if (!fromIn && !toIn && !chosen.has(hash)) continue;

    txs.add(hash);
    if (!edge.succeeded) failed.add(hash);
    if (from && !fromIn) outside.add(from);
    if (to && !toIn) outside.add(to);
    firstBlock = firstBlock === null ? edge.block_number : Math.min(firstBlock, edge.block_number);
    lastBlock = lastBlock === null ? edge.block_number : Math.max(lastBlock, edge.block_number);
    firstTime = firstTime === null ? edge.timestamp : Math.min(firstTime, edge.timestamp);
    lastTime = lastTime === null ? edge.timestamp : Math.max(lastTime, edge.timestamp);

    const transfer = edge.succeeded ? edgeTransfer(edge) : null;
    if (!transfer) continue;
    const flow = flows.get(transfer.key) ?? {
      key: transfer.key,
      symbol: transfer.symbol,
      decimals: transfer.decimals,
      inflow: 0n,
      outflow: 0n,
      internal: 0n,
      volume: 0n,
    };
    flow.volume += transfer.amount;
    if (fromIn && toIn) flow.internal += transfer.amount;
    else if (toIn) flow.inflow += transfer.amount;
    else if (fromIn) flow.outflow += transfer.amount;
    flows.set(transfer.key, flow);
  }

  for (const member of members) outside.delete(member);

  return {
    nodes: group.nodes.length,
    txs: txs.size,
    failed: failed.size,
    counterparties: outside.size,
    firstBlock,
    lastBlock,
    firstTime,
    lastTime,
    flows: [...flows.values()].sort((a, b) => Number(b.volume > a.volume) - Number(b.volume < a.volume)),
  };
}

export function addToGroup(groups: GraphGroup[], target: string | { name: string }, nodes: string[], txs: string[]): GraphGroup[] {
  if (typeof target === 'string') {
    return groups.map((group) => (group.id === target
      ? {
          ...group,
          nodes: [...new Set([...group.nodes, ...nodes])],
          txs: [...new Set([...group.txs, ...txs])],
        }
      : group));
  }
  return [
    ...groups,
    {
      id: newGroupId(),
      name: target.name.trim() || `Group ${groups.length + 1}`,
      color: nextGroupColor(groups),
      nodes: [...new Set(nodes)],
      txs: [...new Set(txs)],
      hidden: false,
    },
  ];
}

export function parseGroups(raw: unknown): GraphGroup[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((item): GraphGroup[] => {
    if (typeof item !== 'object' || item === null) return [];
    const group = item as Record<string, unknown>;
    if (typeof group.id !== 'string' || typeof group.name !== 'string') return [];
    const strings = (value: unknown) => (Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : []);
    return [{
      id: group.id,
      name: group.name,
      color: typeof group.color === 'string' ? group.color : GROUP_COLORS[0],
      nodes: strings(group.nodes),
      txs: strings(group.txs),
      hidden: group.hidden === true,
    }];
  });
}
