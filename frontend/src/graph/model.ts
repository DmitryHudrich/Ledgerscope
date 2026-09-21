import {
  ETH_ASSET,
  contractAddressesOf,
  edgeEndpoints,
  edgeFlavor,
  edgeTransfer,
  type EdgeTransfer,
} from '../api/edges';
import type { GraphEdge, GraphResponse } from '../api/types';
import { weiToEth } from '../lib/format';

export type NodeKind = 'focus' | 'contract' | 'eoa';

export type LinkTone = 'eth' | 'token' | 'mixed' | 'call';

export interface AssetFlow {
  key: string;
  symbol: string;
  decimals: number;
  native: boolean;
  amount: bigint;
  count: number;
}

export interface GraphNode {
  id: string;
  kind: NodeKind;
  depth: number;
  expanded: boolean;
  inCount: number;
  outCount: number;
  valueIn: bigint;
  valueOut: bigint;

  assetsIn: Map<string, AssetFlow>;
  assetsOut: Map<string, AssetFlow>;

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
  assets: AssetFlow[];

  weight: number;

  transfers: number;
  calls: number;
  failed: number;
  tone: LinkTone;
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
  assets: AssetFlow[];
  transfers: number;
  tokenTransfers: number;
  calls: number;
  minBlock: number;
  maxBlock: number;
  firstSeen: number;
  lastSeen: number;

  hiddenNodes: number;
  hiddenTxs: number;
  failedTxs: number;
}

export interface GraphFilters {
  focus: string;

  minEth: number;

  showNative: boolean;

  showTokens: boolean;

  showCalls: boolean;

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

export interface NodePosition {
  x: number;
  y: number;
  vx?: number;
  vy?: number;
  fx?: number | null;
  fy?: number | null;
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
    assets: [],
    transfers: 0,
    tokenTransfers: 0,
    calls: 0,
    minBlock: 0,
    maxBlock: 0,
    firstSeen: 0,
    lastSeen: 0,
    hiddenNodes: 0,
    hiddenTxs: 0,
    failedTxs: 0,
  },
};

export const DEFAULT_FILTERS: GraphFilters = {
  focus: '',
  minEth: 0,
  showNative: true,
  showTokens: true,
  showCalls: true,
  hideIsolated: true,
};

const NODE_MIN_R = 4.5;
const NODE_MAX_GROWTH = 13;
const FOCUS_BONUS = 3;

function blank(id: string, kind: NodeKind): GraphNode {
  return {
    id,
    kind,
    depth: 0,
    expanded: false,
    inCount: 0,
    outCount: 0,
    valueIn: 0n,
    valueOut: 0n,
    assetsIn: new Map(),
    assetsOut: new Map(),
    turnover: 0,
    degree: 0,
    firstBlock: Number.POSITIVE_INFINITY,
    lastBlock: 0,
    firstSeen: Number.POSITIVE_INFINITY,
    lastSeen: 0,
    r: NODE_MIN_R,
    x: Number.NaN,
    y: Number.NaN,
    vx: 0,
    vy: 0,
  };
}

function bump(into: Map<string, AssetFlow>, transfer: EdgeTransfer): void {
  let flow = into.get(transfer.key);
  if (!flow) {
    flow = {
      key: transfer.key,
      symbol: transfer.symbol,
      decimals: transfer.decimals,
      native: transfer.native,
      amount: 0n,
      count: 0,
    };
    into.set(transfer.key, flow);
  }
  flow.amount += transfer.amount;
  flow.count += 1;
}

function merge(into: Map<string, AssetFlow>, flows: Iterable<AssetFlow>): void {
  for (const flow of flows) {
    const current = into.get(flow.key);
    if (current) {
      current.amount += flow.amount;
      current.count += flow.count;
    } else {
      into.set(flow.key, { ...flow });
    }
  }
}

export function sortedAssets(flows: Iterable<AssetFlow>): AssetFlow[] {
  return [...flows].sort((a, b) => {
    if (a.native !== b.native) return a.native ? -1 : 1;
    if (a.count !== b.count) return b.count - a.count;
    return a.symbol.localeCompare(b.symbol);
  });
}

function toneOf(ethCount: number, tokenCount: number, transfers: number): LinkTone {
  if (transfers === 0) return 'call';
  if (ethCount > 0 && tokenCount > 0) return 'mixed';
  return tokenCount > 0 ? 'token' : 'eth';
}

export function buildGraph(
  response: GraphResponse,
  filters: GraphFilters,
  previous?: Map<string, NodePosition>,
): GraphModel {
  const focus = filters.focus.trim().toLowerCase();
  const walked = new Map(
    response.nodes.map((node) => [node.address.toLowerCase(), node] as const),
  );
  const roots = new Set(
    response.nodes.filter((node) => node.root).map((node) => node.address.toLowerCase()),
  );
  const minWei =
    filters.minEth > 0
      ? BigInt(Math.round(filters.minEth * 1e6)) * 10n ** 12n
      : 0n;

  const contracts = new Set<string>();
  for (const edge of response.edges) {
    for (const address of contractAddressesOf(edge)) contracts.add(address.toLowerCase());
  }

  const kindOf = (id: string): NodeKind =>
    id === focus || roots.has(id) ? 'focus' : contracts.has(id) ? 'contract' : 'eoa';

  const byId = new Map<string, GraphNode>();
  const touch = (id: string): GraphNode => {
    let node = byId.get(id);
    if (!node) {
      node = blank(id, kindOf(id));
      const seed = previous?.get(id);
      if (seed) {
        node.x = seed.x;
        node.y = seed.y;
        node.vx = seed.vx ?? 0;
        node.vy = seed.vy ?? 0;
        node.fx = seed.fx;
        node.fy = seed.fy;
      }
      byId.set(id, node);
    }
    return node;
  };

  const bundles = new Map<string, GraphLink>();
  const bundleAssets = new Map<string, Map<string, AssetFlow>>();
  const ethCounts = new Map<string, number>();
  const tokenCounts = new Map<string, number>();
  let hiddenTxs = 0;

  for (const edge of response.edges) {
    const endpoints = edgeEndpoints(edge);
    if (!endpoints) {
      hiddenTxs += 1;
      continue;
    }

    const flavor = edgeFlavor(edge);
    const allowed =
      flavor === 'native'
        ? filters.showNative
        : flavor === 'token'
          ? filters.showTokens
          : filters.showCalls;
    if (!allowed) {
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
        assets: [],
        weight: 0,
        transfers: 0,
        calls: 0,
        failed: 0,
        tone: 'call',
        width: 1,
        curve: 0,
        selfLoop: from === to,
      };
      bundles.set(key, bundle);
      bundleAssets.set(key, new Map());
    }

    bundle.txs.push(edge);
    bundle.count += 1;
    if (!edge.succeeded) bundle.failed += 1;

    const transfer = edge.succeeded ? edgeTransfer(edge) : null;
    if (transfer) {
      bundle.transfers += 1;
      if (transfer.native) {
        bundle.value += transfer.amount;
        ethCounts.set(key, (ethCounts.get(key) ?? 0) + 1);
      } else {
        tokenCounts.set(key, (tokenCounts.get(key) ?? 0) + 1);
      }
      bump(bundleAssets.get(key)!, transfer);
    } else {
      bundle.calls += 1;
    }
  }

  const links: GraphLink[] = [];
  for (const bundle of bundles.values()) {
    const flows = bundleAssets.get(bundle.id)!;
    const carriesToken = [...flows.values()].some((flow) => !flow.native);
    if (minWei > 0n && !carriesToken && bundle.value < minWei) {
      hiddenTxs += bundle.count;
      continue;
    }
    bundle.weight = weiToEth(bundle.value);
    bundle.assets = sortedAssets(flows.values());
    bundle.tone = toneOf(
      ethCounts.get(bundle.id) ?? 0,
      tokenCounts.get(bundle.id) ?? 0,
      bundle.transfers,
    );
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

  const totalAssets = new Map<string, AssetFlow>();
  let volume = 0n;
  let minBlock = Number.POSITIVE_INFINITY;
  let maxBlock = 0;
  let firstSeen = Number.POSITIVE_INFINITY;
  let lastSeen = 0;

  for (const link of links) {
    const source = link.source;
    const target = link.target;

    source.outCount += link.count;
    source.valueOut += link.value;
    target.inCount += link.count;
    target.valueIn += link.value;

    merge(source.assetsOut, link.assets);
    merge(target.assetsIn, link.assets);
    merge(totalAssets, link.assets);
    volume += link.value;

    for (const edge of link.txs) {
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
    for (const raw of response.nodes) touch(raw.address.toLowerCase());
  }

  for (const id of roots) touch(id);

  const nodes = [...byId.values()];
  for (const node of nodes) {
    const seen = walked.get(node.id);
    node.depth = seen?.depth ?? 0;
    node.expanded = seen?.expanded ?? false;
  }
  let maxTurnover = 0;
  let maxDegree = 0;
  let maxNodeTxs = 0;
  let maxLinkWeight = 0;
  let maxLinkCount = 0;
  for (const node of nodes) {
    node.turnover = weiToEth(node.valueIn + node.valueOut);
    node.degree = neighbors.get(node.id)?.size ?? 0;
    if (node.firstBlock === Number.POSITIVE_INFINITY) node.firstBlock = 0;
    if (node.firstSeen === Number.POSITIVE_INFINITY) node.firstSeen = 0;
    if (node.turnover > maxTurnover) maxTurnover = node.turnover;
    if (node.degree > maxDegree) maxDegree = node.degree;
    const txs = node.inCount + node.outCount;
    if (txs > maxNodeTxs) maxNodeTxs = txs;
  }
  for (const link of links) {
    if (link.weight > maxLinkWeight) maxLinkWeight = link.weight;
    if (link.count > maxLinkCount) maxLinkCount = link.count;
  }

  for (const node of nodes) {
    const byValue = maxTurnover > 0 ? Math.sqrt(node.turnover / maxTurnover) : 0;
    const byDegree = maxDegree > 0 ? Math.sqrt(node.degree / maxDegree) : 0;
    const byTxs =
      maxNodeTxs > 0 ? Math.sqrt((node.inCount + node.outCount) / maxNodeTxs) : 0;
    node.r =
      NODE_MIN_R +
      NODE_MAX_GROWTH * Math.max(byValue, byDegree * 0.7, byTxs * 0.55) +
      (node.kind === 'focus' ? FOCUS_BONUS : 0);
  }
  for (const link of links) {
    const byValue = maxLinkWeight > 0 ? Math.sqrt(link.weight / maxLinkWeight) : 0;
    const byCount = maxLinkCount > 0 ? Math.sqrt(link.count / maxLinkCount) : 0;
    link.width = 1 + 4 * Math.max(byValue, byCount * 0.7);
  }

  assignCurves(links);
  seedPositions(nodes);

  const hiddenNodes = Math.max(
    0,
    new Set(response.nodes.map((n) => n.address.toLowerCase())).size - nodes.length,
  );

  const transfers = links.reduce((sum, link) => sum + link.transfers, 0);
  const tokenTransfers = [...totalAssets.values()]
    .filter((flow) => !flow.native)
    .reduce((sum, flow) => sum + flow.count, 0);

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
      assets: sortedAssets(totalAssets.values()),
      transfers,
      tokenTransfers,
      calls: links.reduce((sum, link) => sum + link.calls, 0),
      minBlock: Number.isFinite(minBlock) ? minBlock : 0,
      maxBlock,
      firstSeen: Number.isFinite(firstSeen) ? firstSeen : 0,
      lastSeen,
      hiddenNodes,
      hiddenTxs,
      failedTxs: links.reduce((sum, link) => sum + link.failed, 0),
    },
  };
}

export function netFlow(node: GraphNode): AssetFlow[] {
  const keys = new Set([...node.assetsIn.keys(), ...node.assetsOut.keys()]);
  const out: AssetFlow[] = [];
  for (const key of keys) {
    const inflow = node.assetsIn.get(key);
    const outflow = node.assetsOut.get(key);
    const sample = inflow ?? outflow;
    if (!sample) continue;
    out.push({
      key,
      symbol: sample.symbol,
      decimals: sample.decimals,
      native: sample.native,
      amount: (inflow?.amount ?? 0n) - (outflow?.amount ?? 0n),
      count: (inflow?.count ?? 0) + (outflow?.count ?? 0),
    });
  }
  return sortedAssets(out);
}

export function assetLabel(flow: AssetFlow): string {
  return flow.native ? ETH_ASSET : flow.symbol;
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
    if (Number.isFinite(node.x) && Number.isFinite(node.y)) continue;
    const t = (placed + 0.5) / nodes.length;
    const angle = placed * golden;
    node.x = Math.cos(angle) * radius * Math.sqrt(t);
    node.y = Math.sin(angle) * radius * Math.sqrt(t);
    placed += 1;
  }
}

export function positionsOf(model: GraphModel): Map<string, NodePosition> {
  const out = new Map<string, NodePosition>();
  for (const node of model.nodes) {
    out.set(node.id, {
      x: node.x,
      y: node.y,
      vx: node.vx,
      vy: node.vy,
      fx: node.fx,
      fy: node.fy,
    });
  }
  return out;
}

export function positionsWithNewNodesNear(
  model: GraphModel,
  sourceId: string,
  addresses: Iterable<string>,
): Map<string, NodePosition> {
  const positions = positionsOf(model);
  const source = model.byId.get(sourceId);
  if (!source) return positions;

  const ids = [...new Set([...addresses].map((address) => address.toLowerCase()))]
    .filter((id) => !model.byId.has(id))
    .sort();
  const goldenAngle = Math.PI * (3 - Math.sqrt(5));
  const phase = stablePhase(sourceId);

  ids.forEach((id, index) => {
    const radius = 52 + 18 * Math.sqrt(index + 1);
    const angle = phase + index * goldenAngle;
    positions.set(id, {
      x: source.x + Math.cos(angle) * radius,
      y: source.y + Math.sin(angle) * radius,
      vx: 0,
      vy: 0,
    });
  });

  return positions;
}

function stablePhase(value: string): number {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return ((hash >>> 0) / 4294967296) * Math.PI * 2;
}
