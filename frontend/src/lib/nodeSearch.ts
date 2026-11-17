import type { GraphModel, GraphNode } from '../graph/model';
import { displayAddress, parseAddressInput } from './chain';

export const MIN_QUERY = 2;

export interface NodeHit {
  node: GraphNode;
  score: number;
  reason: string;
}

function hitOf(node: GraphNode, needle: string, exact: string | null): NodeHit | null {
  const id = node.id.toLowerCase();
  if (exact && id === exact) return { node, score: 0, reason: 'address' };

  const bare = needle.replace(/^0x/, '');
  if (bare.length >= MIN_QUERY && id.slice(2).startsWith(bare)) return { node, score: 1, reason: 'address' };

  const shown = displayAddress(node.id);
  if (shown !== node.id && shown.toLowerCase().includes(needle)) return { node, score: 2, reason: 'address' };
  if (id.includes(needle)) return { node, score: 2, reason: 'address' };

  for (const label of node.systemLabels) {
    const value = label.value.toLowerCase();
    if (value.startsWith(needle)) return { node, score: 3, reason: label.value };
    if (value.includes(needle)) return { node, score: 4, reason: label.value };
  }

  const tag = node.tags.find((candidate) => candidate.key.toLowerCase().includes(needle));
  return tag ? { node, score: 5, reason: tag.key } : null;
}

export function searchNodes(model: GraphModel, query: string): NodeHit[] {
  const raw = query.trim();
  const needle = raw.toLowerCase();
  if (needle.length < MIN_QUERY) return [];
  const exact = parseAddressInput(raw);

  const hits: NodeHit[] = [];
  for (const node of model.nodes) {
    const hit = hitOf(node, needle, exact);
    if (hit) hits.push(hit);
  }
  return hits.sort((a, b) => a.score - b.score || b.node.r - a.node.r);
}
