import {
  contractAddressOf,
  edgeEndpoints,
  edgeWei,
  isContractEdge,
} from '../api/edges';
import type { GraphEdge, GraphResponse } from '../api/types';
import { weiToEth } from '../lib/format';

export type NodeKind = 'focus' | 'contract' | 'eoa';

export interface GraphNode {
  id: string;
  kind: NodeKind;
  inCount: number;
  outCount: number;
  valueIn: bigint;
  valueOut: bigint;

  turnover: number;

  degree: number;
  firstBlock: number;
  lastBlock: number;
  firstSeen: number;
  lastSeen: number;
  r: number;

  index?: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx?: number | null;
  fy?: number | null;
}

export interface GraphLink {
  id: string;
  source: GraphNode;
  target: GraphNode;
  txs: GraphEdge[];
  count: number;

  value: bigint;

  weight: number;

  calls: number;
  width: number;

  curve: number;
  selfLoop: boolean;
}

export interface GraphStats {
  nodes: number;
  links: number;
  txs: number;
  contracts: number;
  volume: bigint;
  minBlock: number;
  maxBlock: number;
  firstSeen: number;
  lastSeen: number;

  hiddenNodes: number;
  hiddenTxs: number;
}

export interface GraphFilters {
  focus: string;

  minEth: number;

  showCalls: boolean;

  showTransfers: boolean;

  hideIsolated: boolean;
}

export interface GraphModel {
  nodes: GraphNode[];
  links: GraphLink[];
  byId: Map<string, GraphNode>;
  linksByNode: Map<string, GraphLink[]>;
  neighbors: Map<string, Set<string>>;
  stats: GraphStats;
}

export const EMPTY_MODEL: GraphModel = {
  nodes: [],
  links: [],
  byId: new Map(),
  linksByNode: new Map(),
  neighbors: new Map(),
  stats: {
    nodes: 0,
    links: 0,
    txs: 0,
    contracts: 0,
    volume: 0n,
    minBlock: 0,
    maxBlock: 0,
    firstSeen: 0,
    lastSeen: 0,
    hiddenNodes: 0,
    hiddenTxs: 0,
  },
};

export const DEFAULT_FILTERS: GraphFilters = {
  focus: '',
  minEth: 0,
  showCalls: true,
  showTransfers: true,
  hideIsolated: true,
};

const NODE_MIN_R = 4.5;
const NODE_MAX_GROWTH = 13;
const FOCUS_BONUS = 3;

function blank(id: string, kind: NodeKind): GraphNode {
  return {
    id,
    kind,
    inCount: 0,
    outCount: 0,
    valueIn: 0n,
    valueOut: 0n,
    turnover: 0,
    degree: 0,
    firstBlock: Number.POSITIVE_INFINITY,
    lastBlock: 0,
    firstSeen: Number.POSITIVE_INFINITY,
    lastSeen: 0,
    r: NODE_MIN_R,
    x: 0,
    y: 0,
    vx: 0,
    vy: 0,
  };
}

export function buildGraph(
  response: GraphResponse,
  filters: GraphFilters,
  previous?: Map<string, { x: number; y: number }>,
): GraphModel {
  const focus = filters.focus.trim().toLowerCase();
  const minWei =
    filters.minEth > 0
      ? BigInt(Math.round(filters.minEth * 1e6)) * 10n ** 12n
      : 0n;

  const contracts = new Set<string>();
  for (const edge of response.edges) {
    const address = contractAddressOf(edge);
    if (address) contracts.add(address.toLowerCase());
  }

  const kindOf = (id: string): NodeKind =>
    id === focus ? 'focus' : contracts.has(id) ? 'contract' : 'eoa';

  const byId = new Map<string, GraphNode>();
  const touch = (id: string): GraphNode => {
    let node = byId.get(id);
    if (!node) {
      node = blank(id, kindOf(id));
      const seed = previous?.get(id);
      if (seed) {
        node.x = seed.x;
        node.y = seed.y;
      }
      byId.set(id, node);
    }
    return node;
  };

  const bundles = new Map<string, GraphLink>();
  let volume = 0n;
  let minBlock = Number.POSITIVE_INFINITY;
  let maxBlock = 0;
  let firstSeen = Number.POSITIVE_INFINITY;
  let lastSeen = 0;
  let hiddenTxs = 0;

  for (const edge of response.edges) {

    const endpoints = edgeEndpoints(edge);
    if (!endpoints) {
      hiddenTxs += 1;
      continue;
    }
    const isCall = isContractEdge(edge);
    if (isCall ? !filters.showCalls : !filters.showTransfers) {
      hiddenTxs += 1;
      continue;
    }

    const from = endpoints.from.toLowerCase();
    const to = endpoints.to.toLowerCase();
    const key = `${from}>${to}`;
    let bundle = bundles.get(key);
    if (!bundle) {
      bundle = {
        id: key,
        source: touch(from),
        target: touch(to),
        txs: [],
        count: 0,
        value: 0n,
        weight: 0,
        calls: 0,
        width: 1,
        curve: 0,
        selfLoop: from === to,
      };
      bundles.set(key, bundle);
    }
    bundle.txs.push(edge);
    bundle.count += 1;
    bundle.value += edgeWei(edge);
    if (isCall) bundle.calls += 1;
  }

  const links: GraphLink[] = [];
  for (const bundle of bundles.values()) {
    if (minWei > 0n && bundle.value < minWei) {
      hiddenTxs += bundle.count;
      continue;
    }
    bundle.weight = weiToEth(bundle.value);
    links.push(bundle);
  }

  const kept = new Set<string>();
  const neighbors = new Map<string, Set<string>>();
  const linksByNode = new Map<string, GraphLink[]>();
  const attach = (id: string, link: GraphLink) => {
    kept.add(id);
    const list = linksByNode.get(id);
    if (list) list.push(link);
    else linksByNode.set(id, [link]);
  };
  const relate = (a: string, b: string) => {
    const set = neighbors.get(a);
    if (set) set.add(b);
    else neighbors.set(a, new Set([b]));
  };

  for (const link of links) {
    const source = link.source;
    const target = link.target;

    source.outCount += link.count;
    source.valueOut += link.value;
    target.inCount += link.count;
    target.valueIn += link.value;

    for (const edge of link.txs) {
      volume += edgeWei(edge);
      if (edge.block_number < minBlock) minBlock = edge.block_number;
      if (edge.block_number > maxBlock) maxBlock = edge.block_number;
      if (edge.timestamp && edge.timestamp < firstSeen) firstSeen = edge.timestamp;
      if (edge.timestamp > lastSeen) lastSeen = edge.timestamp;
      for (const node of [source, target]) {
        if (edge.block_number < node.firstBlock) node.firstBlock = edge.block_number;
        if (edge.block_number > node.lastBlock) node.lastBlock = edge.block_number;
        if (edge.timestamp && edge.timestamp < node.firstSeen) node.firstSeen = edge.timestamp;
        if (edge.timestamp > node.lastSeen) node.lastSeen = edge.timestamp;
      }
    }

    attach(source.id, link);
    if (target.id !== source.id) attach(target.id, link);
    relate(source.id, target.id);
    relate(target.id, source.id);
  }

  if (filters.hideIsolated) {
    for (const [id, node] of byId) {
      if (!kept.has(id) && node.kind !== 'focus') byId.delete(id);
    }
  } else {
    for (const raw of response.nodes) touch(raw.toLowerCase());
  }

  const nodes = [...byId.values()];
  let maxTurnover = 0;
  let maxDegree = 0;
  let maxLinkWeight = 0;
  for (const node of nodes) {
    node.turnover = weiToEth(node.valueIn + node.valueOut);
    node.degree = neighbors.get(node.id)?.size ?? 0;
    if (node.firstBlock === Number.POSITIVE_INFINITY) node.firstBlock = 0;
    if (node.firstSeen === Number.POSITIVE_INFINITY) node.firstSeen = 0;
    if (node.turnover > maxTurnover) maxTurnover = node.turnover;
    if (node.degree > maxDegree) maxDegree = node.degree;
  }
  for (const link of links) {
    if (link.weight > maxLinkWeight) maxLinkWeight = link.weight;
  }

  for (const node of nodes) {
    const byValue = maxTurnover > 0 ? Math.sqrt(node.turnover / maxTurnover) : 0;
    const byDegree = maxDegree > 0 ? Math.sqrt(node.degree / maxDegree) : 0;
    node.r =
      NODE_MIN_R +
      NODE_MAX_GROWTH * Math.max(byValue, byDegree * 0.7) +
      (node.kind === 'focus' ? FOCUS_BONUS : 0);
  }
  for (const link of links) {
    const t = maxLinkWeight > 0 ? Math.sqrt(link.weight / maxLinkWeight) : 0;
    link.width = 1 + 4 * t;
  }

  assignCurves(links);
  seedPositions(nodes);

  const hiddenNodes = Math.max(0, new Set(response.nodes.map((n) => n.toLowerCase())).size - nodes.length);

  return {
    nodes,
    links,
    byId,
    linksByNode,
    neighbors,
    stats: {
      nodes: nodes.length,
      links: links.length,
      txs: links.reduce((sum, link) => sum + link.count, 0),
      contracts: nodes.filter((n) => n.kind === 'contract').length,
      volume,
      minBlock: Number.isFinite(minBlock) ? minBlock : 0,
      maxBlock,
      firstSeen: Number.isFinite(firstSeen) ? firstSeen : 0,
      lastSeen,
      hiddenNodes,
      hiddenTxs,
    },
  };
}

function assignCurves(links: GraphLink[]): void {
  const pairs = new Map<string, number>();
  for (const link of links) {
    const a = link.source.id;
    const b = link.target.id;
    const key = a < b ? `${a}|${b}` : `${b}|${a}`;
    pairs.set(key, (pairs.get(key) ?? 0) + 1);
  }
  for (const link of links) {
    if (link.selfLoop) continue;
    const a = link.source.id;
    const b = link.target.id;
    const key = a < b ? `${a}|${b}` : `${b}|${a}`;
    link.curve = (pairs.get(key) ?? 1) > 1 ? (a < b ? 0.17 : -0.17) : 0;
  }
}

function seedPositions(nodes: GraphNode[]): void {
  const radius = 14 * Math.sqrt(Math.max(nodes.length, 1));
  const golden = Math.PI * (3 - Math.sqrt(5));
  let placed = 0;
  for (const node of nodes) {
    if (node.x !== 0 || node.y !== 0) continue;
    const t = (placed + 0.5) / nodes.length;
    const angle = placed * golden;
    node.x = Math.cos(angle) * radius * Math.sqrt(t);
    node.y = Math.sin(angle) * radius * Math.sqrt(t);
    placed += 1;
  }
}

export function positionsOf(model: GraphModel): Map<string, { x: number; y: number }> {
  const out = new Map<string, { x: number; y: number }>();
  for (const node of model.nodes) out.set(node.id, { x: node.x, y: node.y });
  return out;
}
