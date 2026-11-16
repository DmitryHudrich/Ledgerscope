import { formatCount } from './format';

export type RangeSource = 'auto' | 'rpc' | 'dataset';

export interface BlockSpan {
  from: number;
  to: number;
  source?: RangeSource;
  exact?: boolean;
}

export interface BlockLimits {
  head: number | null;
  maxBlocks: number;
}

export type RangeEdge = 'from' | 'to';

function within(value: number): number {
  return Math.max(Math.trunc(value), 0);
}

export function clampRange(next: BlockSpan, limits: BlockLimits, edited: RangeEdge = 'to'): BlockSpan {
  const span = Math.max(limits.maxBlocks, 1);
  let from = within(next.from);
  let to = within(next.to);

  if (from > to) {
    if (edited === 'from') to = from;
    else from = to;
  }

  if (to - from + 1 > span) {
    if (edited === 'from') to = from + span - 1;
    else from = Math.max(to - span + 1, 0);
  }

  return { ...next, from, to };
}

export function sameRange(a: BlockSpan, b: BlockSpan): boolean {
  return a.from === b.from && a.to === b.to;
}

export function rangeProblem(next: BlockSpan, limits: BlockLimits): string | null {
  if (!Number.isFinite(next.from) || !Number.isFinite(next.to) || next.from < 0 || next.to < 0) {
    return 'Enter a non-negative block number.';
  }
  if (next.from > next.to) {
    return 'From block cannot be greater than To block.';
  }
  if (next.to - next.from + 1 > limits.maxBlocks) {
    return `At most ${formatCount(limits.maxBlocks)} blocks per graph, this range has ${formatCount(next.to - next.from + 1)}.`;
  }
  return null;
}

export function indexedWithin(ranges: Array<{ from_block: number; to_block: number }>, maxBlocks: number): BlockSpan[] {
  const picked: BlockSpan[] = [];
  let room = Math.max(maxBlocks, 0);
  for (const range of [...ranges].sort((a, b) => b.to_block - a.to_block)) {
    if (room <= 0) break;
    const from = Math.max(range.from_block, range.to_block - room + 1);
    picked.push({ from, to: range.to_block });
    room -= range.to_block - from + 1;
  }
  return picked.sort((a, b) => a.from - b.from);
}
