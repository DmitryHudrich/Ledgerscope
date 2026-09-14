import type { GraphResponse, TxResponse } from '../api/types';
import { hasCalldata, parseWei, weiToEth } from '../lib/format';

/**
 * `focus` is the wallet the query was seeded from; `contract` is inferred from
 * calldata landing on the address (the API exposes no code flag).
 */
export type NodeKind = 'focus' | 'contract' | 'eoa';

export interface GraphNode {
  id: string;
  kind: NodeKind;
  inCount: number;
  outCount: number;
  valueIn: bigint;
  valueOut: bigint;
  /** ETH turnover (in + out), what node area encodes. */
  turnover: number;
  /** Distinct counterparties. */
  degree: number;
  firstBlock: number;
  lastBlock: number;
  firstSeen: number;
  lastSeen: number;
  r: number;
  // Written by d3-force.
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
  txs: TxResponse[];
  count: number;
  value: bigint;
  /** ETH moved across the whole bundle, what stroke width encodes. */
  weight: number;
  /** Bundle carrying calldata (contract calls) rather than plain transfers. */
  calls: number;
  width: number;
  /** Bow of the quadratic curve; opposite signs separate reciprocal flows. */
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
  /** Nodes/txs dropped by the active filters. */
  hiddenNodes: number;
  hiddenTxs: number;
}

export interface GraphFilters {
  focus: string;
  /** Drop bundles moving less than this many ETH. */
  minEth: number;
  /** Include transactions carrying calldata. */
  showCalls: boolean;
  /** Include plain value transfers. */
  showTransfers: boolean;
  /** Drop addresses left without a visible edge. */
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

/**
 * Folds the raw tx list into a node-link model: one bundle per ordered
 * (from -> to) pair, carrying its transactions for the detail views.
 *
 * `previous` seeds coordinates for addresses that survive a filter change, so
 * re-filtering nudges the layout instead of reshuffling it.
 */
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

  // Calldata anywhere on an address marks it a contract, even if the tx that
  // proved it is later filtered out.
  const contracts = new Set<string>();
  for (const tx of response.edges) {
    if (tx.to && hasCalldata(tx.data)) contracts.add(tx.to.toLowerCase());
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

  for (const tx of response.edges) {
    // Contract creations have no recipient and therefore no edge to draw.
    if (!tx.to) {
      hiddenTxs += 1;
      continue;
    }
    const isCall = hasCalldata(tx.data);
    if (isCall ? !filters.showCalls : !filters.showTransfers) {
      hiddenTxs += 1;
      continue;
    }

    const from = tx.from.toLowerCase();
    const to = tx.to.toLowerCase();
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
    bundle.txs.push(tx);
    bundle.count += 1;
    bundle.value += parseWei(tx.amount);
    if (isCall) bundle.calls += 1;
  }

  // Value threshold applies to the bundle, not the single tx — a stream of dust
  // between two addresses is exactly the pattern worth keeping visible.
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

    for (const tx of link.txs) {
      volume += parseWei(tx.amount);
      if (tx.block_number < minBlock) minBlock = tx.block_number;
      if (tx.block_number > maxBlock) maxBlock = tx.block_number;
      if (tx.timestamp && tx.timestamp < firstSeen) firstSeen = tx.timestamp;
      if (tx.timestamp > lastSeen) lastSeen = tx.timestamp;
      for (const node of [source, target]) {
        if (tx.block_number < node.firstBlock) node.firstBlock = tx.block_number;
        if (tx.block_number > node.lastBlock) node.lastBlock = tx.block_number;
        if (tx.timestamp && tx.timestamp < node.firstSeen) node.firstSeen = tx.timestamp;
        if (tx.timestamp > node.lastSeen) node.lastSeen = tx.timestamp;
      }
    }

    attach(source.id, link);
    if (target.id !== source.id) attach(target.id, link);
    relate(source.id, target.id);
    relate(target.id, source.id);
  }

  // Addresses the API listed that no surviving edge touches — contract
  // creations and anything the filters emptied out.
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

  // Area-proportional-ish sizing on turnover, with a floor from connectivity so
  // a busy zero-value hub is still a visible node.
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

/** Reciprocal bundles bow to opposite sides so neither hides the other. */
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

/**
 * d3 only auto-places nodes whose coordinates are NaN, and ours start at the
 * origin, so unseeded nodes get a phyllotaxis spiral — deterministic, evenly
 * spread, and it unfolds without the first-tick explosion.
 */
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
