import type { GraphResponse } from './types';

export interface GraphMergeResult {
  response: GraphResponse;
  addedNodes: number;
  addedTransactions: number;
}

export function mergeGraphResponses(
  current: GraphResponse,
  incoming: GraphResponse,
): GraphMergeResult {
  const nodes = new Map<string, GraphResponse['nodes'][number]>();
  for (const node of current.nodes) nodes.set(node.address.toLowerCase(), node);

  let addedNodes = 0;
  for (const node of incoming.nodes) {
    const id = node.address.toLowerCase();
    if (!nodes.has(id)) addedNodes += 1;
    nodes.set(id, node);
  }

  const hashes = new Set<string>();
  const edges = [] as GraphResponse['edges'];
  for (const edge of current.edges) {
    const key = edge.tx_hash.toLowerCase();
    if (hashes.has(key)) continue;
    hashes.add(key);
    edges.push(edge);
  }

  let addedTransactions = 0;
  for (const edge of incoming.edges) {
    const key = edge.tx_hash.toLowerCase();
    if (hashes.has(key)) continue;
    hashes.add(key);
    edges.push(edge);
    addedTransactions += 1;
  }

  return {
    response: {
      nodes: [...nodes.values()],
      edges,
      span: incoming.span,
      filled_from_rpc: [...current.filled_from_rpc, ...incoming.filled_from_rpc],
      truncated: current.truncated || incoming.truncated,
    },
    addedNodes,
    addedTransactions,
  };
}
