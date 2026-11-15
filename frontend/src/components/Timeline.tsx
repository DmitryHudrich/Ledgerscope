import { useEffect, useMemo, useState, type ChangeEvent } from 'react';

import type { GraphEdge } from '../api/types';
import type { BlockWindow } from '../graph/model';
import { formatCount, formatTimestamp } from '../lib/format';
import { Button, Panel, cx } from './ui';

export interface BlockBounds {
  from: number;
  to: number;
}

const BUCKETS = 72;

const thumb = cx(
  'pointer-events-none absolute inset-x-0 top-0 m-0 h-5 w-full appearance-none bg-transparent outline-none',
  '[&::-webkit-slider-thumb]:pointer-events-auto [&::-webkit-slider-thumb]:size-3.5 [&::-webkit-slider-thumb]:cursor-ew-resize [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-surface-1 [&::-webkit-slider-thumb]:bg-accent',
  '[&::-moz-range-thumb]:pointer-events-auto [&::-moz-range-thumb]:size-3 [&::-moz-range-thumb]:cursor-ew-resize [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:border-2 [&::-moz-range-thumb]:border-surface-1 [&::-moz-range-thumb]:bg-accent',
  'focus-visible:[&::-webkit-slider-thumb]:ring-2 focus-visible:[&::-webkit-slider-thumb]:ring-accent',
);

interface Props {
  edges: GraphEdge[];
  window: BlockWindow | null;
  onWindowChange: (next: BlockWindow | null) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

function firstAbove(sorted: Float64Array, value: number): number {
  let low = 0;
  let high = sorted.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (sorted[middle] <= value) low = middle + 1;
    else high = middle;
  }
  return low;
}

function BlockInput({ id, value, low, high, onCommit }: {
  id: string;
  value: number;
  low: number;
  high: number;
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);

  const commit = () => {
    const parsed = Number.parseInt(draft, 10);
    if (Number.isFinite(parsed)) onCommit(clamp(parsed, low, high));
    else setDraft(String(value));
  };

  return (
    <input
      id={id}
      className="h-7 w-[104px] rounded-ui-sm border border-hairline bg-surface-2 px-2 font-mono-ui text-[11px] tabular-nums text-text-primary outline-none focus-visible:border-accent"
      inputMode="numeric"
      value={draft}
      onChange={(event) => setDraft(event.target.value.replace(/\D/g, ''))}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') commit();
        if (event.key === 'Escape') setDraft(String(value));
      }}
    />
  );
}

export function Timeline({ edges, window, onWindowChange, open, onOpenChange }: Props) {
  const domain = useMemo(() => {
    if (edges.length === 0) return null;
    let from = Number.POSITIVE_INFINITY;
    let to = 0;
    for (const edge of edges) {
      if (edge.block_number < from) from = edge.block_number;
      if (edge.block_number > to) to = edge.block_number;
    }
    return { from, to };
  }, [edges]);

  const stamps = useMemo(() => {
    const byBlock = new Map<number, number>();
    for (const edge of edges) if (edge.timestamp) byBlock.set(edge.block_number, edge.timestamp);
    return [...byBlock.entries()].sort((a, b) => a[0] - b[0]);
  }, [edges]);

  const bars = useMemo(() => {
    if (!domain) return [];
    const span = domain.to - domain.from + 1;
    const count = Math.min(BUCKETS, span);
    const size = span / count;
    const out = Array.from({ length: count }, (_, index) => ({
      from: domain.from + Math.floor(index * size),
      to: domain.from + Math.floor((index + 1) * size) - 1,
      txs: 0,
    }));
    for (const edge of edges) {
      const index = Math.min(Math.floor((edge.block_number - domain.from) / size), count - 1);
      out[index].txs += 1;
    }
    return out;
  }, [domain, edges]);

  const peak = useMemo(() => bars.reduce((top, bar) => Math.max(top, bar.txs), 0), [bars]);
  const from = domain ? clamp(window?.from ?? domain.from, domain.from, domain.to) : 0;
  const to = domain ? clamp(window?.to ?? domain.to, domain.from, domain.to) : 0;
  const blocks = useMemo(() => Float64Array.from(edges, (edge) => edge.block_number).sort(), [edges]);
  const shown = firstAbove(blocks, to) - firstAbove(blocks, from - 1);

  if (!domain) return null;
  const span = Math.max(domain.to - domain.from, 1);

  const stampOf = (block: number): number | null => {
    let low = 0;
    let high = stamps.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (stamps[middle][0] <= block) low = middle + 1;
      else high = middle;
    }
    return stamps[Math.max(low - 1, 0)]?.[1] ?? null;
  };

  const apply = (next: BlockWindow) => {
    const ordered = { from: Math.min(next.from, next.to), to: Math.max(next.from, next.to) };
    onWindowChange(ordered.from <= domain.from && ordered.to >= domain.to ? null : ordered);
  };

  const fromStamp = stampOf(from);
  const toStamp = stampOf(to);

  if (!open) {
    return (
      <Panel className="absolute bottom-[calc(var(--sheet-h)+12px)] left-1/2 z-12 flex -translate-x-1/2 items-center gap-2 px-2 py-1 text-xs text-text-secondary">
        <Button variant="ghost" className="h-7 px-2" onClick={() => onOpenChange(true)}>Timeline</Button>
        {window && (
          <span className="tabular-nums">
            {formatCount(from)}–{formatCount(to)}
          </span>
        )}
      </Panel>
    );
  }

  return (
    <Panel
      className="absolute bottom-[calc(var(--sheet-h)+12px)] left-[calc(var(--investigation-w)+12px)] right-[calc(var(--inspector-w)+12px)] z-12 mx-auto grid max-w-[880px] gap-2 px-3 py-2.5 text-xs text-text-secondary"
      aria-label="Display timeline"
    >
      <div className="flex flex-wrap items-center gap-2">
        <label className="text-[10px] uppercase tracking-[0.07em] text-text-muted" htmlFor="tl-from">Show blocks</label>
        <BlockInput id="tl-from" value={from} low={domain.from} high={to} onCommit={(value) => apply({ from: value, to })} />
        <span aria-hidden="true">–</span>
        <BlockInput id="tl-to" value={to} low={from} high={domain.to} onCommit={(value) => apply({ from, to: value })} />
        <span className="tabular-nums text-text-muted max-[720px]:hidden">
          {fromStamp ? formatTimestamp(fromStamp) : ''}{fromStamp && toStamp ? ' → ' : ''}{toStamp ? formatTimestamp(toStamp) : ''}
        </span>
        <span className="ml-auto tabular-nums">
          <strong className="font-semibold text-text-primary">{formatCount(shown)}</strong> of {formatCount(edges.length)} txs
        </span>
        <Button variant="ghost" className="h-7 px-2" disabled={!window} onClick={() => onWindowChange(null)}>All</Button>
        <Button variant="ghost" className="h-7 px-2" onClick={() => onOpenChange(false)} aria-label="Collapse timeline">Hide</Button>
      </div>

      <div className="relative">
        <div className="flex h-9 items-end gap-px" aria-hidden="true">
          {bars.map((bar) => {
            const inside = bar.to >= from && bar.from <= to;
            return (
              <button
                type="button"
                tabIndex={-1}
                key={bar.from}
                className={cx('min-w-0 flex-1 cursor-pointer rounded-t-[2px] border-0 p-0', inside ? 'bg-accent' : 'bg-hairline-strong opacity-60')}
                style={{ height: `${peak > 0 ? Math.max((bar.txs / peak) * 100, bar.txs > 0 ? 6 : 2) : 2}%` }}
                title={`${formatCount(bar.from)}–${formatCount(bar.to)}: ${formatCount(bar.txs)} txs`}
                onClick={() => apply({ from: bar.from, to: bar.to })}
              />
            );
          })}
        </div>
        <div className="relative mt-1 h-5">
          <div className="absolute inset-x-0 top-[9px] h-0.5 rounded-full bg-hairline" aria-hidden="true" />
          <div
            className="absolute top-[9px] h-0.5 rounded-full bg-accent"
            style={{ left: `${((from - domain.from) / span) * 100}%`, right: `${100 - ((to - domain.from) / span) * 100}%` }}
            aria-hidden="true"
          />
          <input
            type="range"
            className={thumb}
            aria-label="First block shown"
            min={domain.from}
            max={domain.to}
            value={from}
            onChange={(event: ChangeEvent<HTMLInputElement>) => apply({ from: Math.min(Number(event.target.value), to), to })}
          />
          <input
            type="range"
            className={thumb}
            aria-label="Last block shown"
            min={domain.from}
            max={domain.to}
            value={to}
            onChange={(event: ChangeEvent<HTMLInputElement>) => apply({ from, to: Math.max(Number(event.target.value), from) })}
          />
        </div>
      </div>
    </Panel>
  );
}
