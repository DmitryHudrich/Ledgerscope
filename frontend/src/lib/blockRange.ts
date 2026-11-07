import { formatCount } from './format';

export interface BlockSpan {
  from: number;
  to: number;
}

export interface BlockLimits {
  head: number | null;
  maxBlocks: number;
}

export type RangeEdge = 'from' | 'to';

function ceiling(limits: BlockLimits): number {
  return limits.head ?? Number.MAX_SAFE_INTEGER;
}

function within(value: number, limits: BlockLimits): number {
  return Math.min(Math.max(Math.trunc(value), 0), ceiling(limits));
}

export function clampRange(next: BlockSpan, limits: BlockLimits, edited: RangeEdge = 'to'): BlockSpan {
  const span = Math.max(limits.maxBlocks, 1);
  let from = within(next.from, limits);
  let to = within(next.to, limits);

  if (from > to) {
    if (edited === 'from') to = from;
    else from = to;
  }

  if (to - from + 1 > span) {
    if (edited === 'from') to = Math.min(from + span - 1, ceiling(limits));
    else from = Math.max(to - span + 1, 0);
  }

  return { from, to };
}

export function sameRange(a: BlockSpan, b: BlockSpan): boolean {
  return a.from === b.from && a.to === b.to;
}

export function rangeProblem(next: BlockSpan, limits: BlockLimits): string | null {
  if (!Number.isFinite(next.from) || !Number.isFinite(next.to) || next.from < 0 || next.to < 0) {
    return 'Enter a non-negative block number.';
  }
  if (limits.head !== null && (next.from > limits.head || next.to > limits.head)) {
    return `The chain head is ${formatCount(limits.head)}, blocks past it do not exist yet.`;
  }
  if (next.from > next.to) {
    return 'From block cannot be greater than To block.';
  }
  if (next.to - next.from + 1 > limits.maxBlocks) {
    return `At most ${formatCount(limits.maxBlocks)} blocks per graph, this range has ${formatCount(next.to - next.from + 1)}.`;
  }
  return null;
}
