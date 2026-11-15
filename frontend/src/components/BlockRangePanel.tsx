import { useMemo, type CSSProperties } from 'react';

import type { CoverageResponse, HistogramResponse } from '../api/types';
import { indexedWithin, sameRange, type BlockLimits } from '../lib/blockRange';
import { formatCount } from '../lib/format';
import { IconClose } from './Icons';
import { Button, Input, PanelSection, PanelTitle, cx } from './ui';

export interface BlockSpan {
  from: number;
  to: number;
}

interface Props {
  ranges: BlockSpan[];
  activeIndex: number;
  coverage: CoverageResponse | null;
  histogram: HistogramResponse | null;
  limits: BlockLimits;
  onActiveChange: (index: number) => void;
  onRangesChange: (ranges: BlockSpan[]) => void;
  onApply: () => void;
  applyDisabled?: boolean;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

function overlap(from: number, to: number, selection: BlockSpan): boolean {
  return from <= selection.to && to >= selection.from;
}

const digitsOnly = (value: string) => value.replace(/\D/g, '');

export function BlockRangePanel({
  ranges,
  activeIndex,
  coverage,
  histogram,
  onActiveChange,
  onRangesChange,
  limits,
  onApply,
  applyDisabled,
}: Props) {
  const active = ranges[activeIndex] ?? ranges[0] ?? { from: 0, to: 0 };
  const { from, to } = active;

  const histogramSpan = histogram?.span;
  const histogramPeak = useMemo(
    () => histogram?.buckets.reduce((peak, bucket) => Math.max(peak, bucket.indexed_blocks), 0) ?? 0,
    [histogram],
  );
  const selectedWindow = useMemo(() => {
    if (!histogramSpan) return null;
    const span = Math.max(histogramSpan.to_block - histogramSpan.from_block + 1, 1);
    const left = ((from - histogramSpan.from_block) / span) * 100;
    const right = ((to - histogramSpan.from_block + 1) / span) * 100;
    return { left: clamp(left, 0, 100), right: clamp(right, 0, 100) };
  }, [from, histogramSpan, to]);

  const patchRange = (index: number, patch: Partial<BlockSpan>) =>
    onRangesChange(ranges.map((item, idx) => (idx === index ? { ...item, ...patch } : item)));

  const removeRange = (index: number) => {
    if (ranges.length <= 1) return;
    onRangesChange(ranges.filter((_, idx) => idx !== index));
    if (activeIndex >= index) onActiveChange(Math.max(0, activeIndex - 1));
  };

  const addRange = () => {
    onRangesChange([...ranges, { from, to }]);
    onActiveChange(ranges.length);
  };

  const selectedCount = to - from + 1;

  return (
    <PanelSection className="border-b-0 px-3 py-4">
      <div className="mb-2 flex items-center justify-between">
        <PanelTitle className="mb-0">Scan ranges</PanelTitle>
        <span className="text-[11px] tabular-nums text-text-muted">{ranges.length}</span>
      </div>
      <p className="m-0 mb-2 text-[11px] leading-relaxed text-text-muted">
        One range by default. Add more to scan sparse activity — gaps between ranges are skipped. Click a row to edit it with the slider below.
      </p>

      <div className="grid gap-1">
        {ranges.map((range, index) => {
          const isActive = index === activeIndex;
          return (
            <div
              key={index}
              className={cx(
                'flex items-center gap-1.5 rounded-ui-sm px-1.5 py-1',
                isActive ? 'bg-[color-mix(in_srgb,var(--accent)_12%,transparent)]' : 'hover:bg-[color-mix(in_srgb,var(--text-primary)_5%,transparent)]',
              )}
              onPointerDown={() => onActiveChange(index)}
            >
              <Input
                className="h-[28px] w-full font-mono-ui text-[11px]"
                inputMode="numeric"
                placeholder="from"
                value={String(range.from)}
                onChange={(event) => patchRange(index, { from: Number(digitsOnly(event.target.value)) || 0 })}
              />
              <span className="text-text-muted">–</span>
              <Input
                className="h-[28px] w-full font-mono-ui text-[11px]"
                inputMode="numeric"
                placeholder="to"
                value={String(range.to)}
                onChange={(event) => patchRange(index, { to: Number(digitsOnly(event.target.value)) || 0 })}
              />
              <Button
                variant="ghost"
                icon
                className="size-6 flex-none"
                aria-label="Remove range"
                disabled={ranges.length <= 1}
                onClick={() => removeRange(index)}
              >
                <IconClose size={12} />
              </Button>
            </div>
          );
        })}
      </div>
      <Button variant="ghost" className="mt-1.5 h-7 px-2 text-xs" onClick={addRange}>
        + Add range
      </Button>

      <IndexedRanges
        coverage={coverage}
        maxBlocks={limits.maxBlocks}
        ranges={ranges}
        onUse={(range) => patchRange(activeIndex, range)}
        onAdd={(range) => {
          onRangesChange([...ranges, range]);
          onActiveChange(ranges.length);
        }}
        onUseAll={(picked) => {
          onRangesChange(picked);
          onActiveChange(0);
        }}
      />

      <section className="mt-3 border-t border-hairline pt-3" aria-label="Indexed coverage">
        <div className="mb-1.5 flex items-center justify-between text-[10px] uppercase tracking-[0.07em] text-text-muted">
          <span>Active range coverage</span>
          {coverage?.persisted === false && <span>Storage off</span>}
        </div>
        <div
          className="relative flex h-10 items-end gap-px overflow-hidden rounded-ui-sm border border-hairline bg-plane px-1 py-1"
          title="Blocks available locally. The outlined interval is the active range."
        >
          {histogram?.buckets.map((bucket) => {
            const indexed = bucket.indexed_blocks > 0;
            const selected = overlap(bucket.from_block, bucket.to_block, active);
            const height = indexed && histogramPeak > 0 ? Math.max(18, (bucket.indexed_blocks / histogramPeak) * 100) : 10;
            return (
              <span
                key={bucket.from_block}
                className={cx(
                  'min-w-0 flex-1 rounded-[1px] transition-opacity',
                  indexed ? 'bg-accent' : 'bg-hairline-strong',
                  !selected && 'opacity-35',
                )}
                style={{ height: `${height}%` }}
                title={`${formatCount(bucket.from_block)}–${formatCount(bucket.to_block)}: ${formatCount(bucket.indexed_blocks)} indexed blocks`}
              />
            );
          })}
          {!histogram && <span className="grid h-full w-full place-items-center text-[11px] text-text-muted">Coverage unavailable</span>}
          {selectedWindow && selectedWindow.right > selectedWindow.left && (
            <span
              className="pointer-events-none absolute bottom-0 top-0 rounded-[2px] border border-[color-mix(in_srgb,var(--accent)_70%,transparent)] bg-[color-mix(in_srgb,var(--accent)_8%,transparent)]"
              style={{ left: `${selectedWindow.left}%`, width: `${selectedWindow.right - selectedWindow.left}%` } as CSSProperties}
              aria-hidden="true"
            />
          )}
        </div>
      </section>

      <div className="mt-3 flex items-center justify-between border-t border-hairline pt-2 text-[11px] text-text-muted">
        <span>Active: <strong className="font-medium tabular-nums text-text-secondary">{formatCount(selectedCount)}</strong> blocks{coverage?.chain_head != null ? <> · head {formatCount(coverage.chain_head)}</> : null}</span>
        <Button variant="primary" className="h-7 px-3 text-xs" disabled={applyDisabled} onClick={onApply}>
          Apply
        </Button>
      </div>
    </PanelSection>
  );
}

const SHOWN_INDEXED = 12;

function IndexedRanges({ coverage, maxBlocks, ranges, onUse, onAdd, onUseAll }: {
  coverage: CoverageResponse | null;
  maxBlocks: number;
  ranges: BlockSpan[];
  onUse: (range: BlockSpan) => void;
  onAdd: (range: BlockSpan) => void;
  onUseAll: (ranges: BlockSpan[]) => void;
}) {
  const indexed = useMemo(
    () => [...(coverage?.ranges ?? [])].sort((a, b) => b.to_block - a.to_block),
    [coverage],
  );
  if (indexed.length === 0) return null;

  const all = indexedWithin(indexed, maxBlocks);
  const total = indexed.reduce((sum, range) => sum + range.block_count, 0);
  const clipped = all.reduce((sum, range) => sum + range.to - range.from + 1, 0) < total;
  const usingAll = all.length === ranges.length && all.every((range, index) => sameRange(range, ranges[index]));

  return (
    <section className="mt-3 border-t border-hairline pt-3" aria-label="Indexed ranges">
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <span className="text-[10px] uppercase tracking-[0.07em] text-text-muted">
          Already indexed · {formatCount(total)} blocks
        </span>
        <Button
          variant="ghost"
          className="h-6 px-2 text-[11px]"
          disabled={usingAll}
          title={clipped ? `Newest ${formatCount(maxBlocks)} indexed blocks, the per-graph limit` : 'Every indexed range, no RPC needed'}
          onClick={() => onUseAll(all)}
        >
          {clipped ? 'Use newest' : 'Use all'}
        </Button>
      </div>
      <div className="grid gap-0.5">
        {indexed.slice(0, SHOWN_INDEXED).map((range) => {
          const span = { from: range.from_block, to: Math.min(range.to_block, range.from_block + maxBlocks - 1) };
          const picked = ranges.some((current) => sameRange(current, span));
          return (
            <div key={range.from_block} className="flex items-center gap-1 rounded-ui-sm px-1.5 py-0.5 text-[11px] hover:bg-[color-mix(in_srgb,var(--text-primary)_5%,transparent)]">
              <span className={cx('size-1.5 flex-none rounded-full', picked ? 'bg-accent' : 'bg-hairline-strong')} aria-hidden="true" />
              <span className="min-w-0 flex-1 truncate font-mono-ui tabular-nums text-text-secondary">
                {formatCount(range.from_block)}–{formatCount(range.to_block)}
              </span>
              <span className="tabular-nums text-text-muted">{formatCount(range.block_count)}</span>
              <Button variant="ghost" className="h-6 px-1.5 text-[11px]" disabled={picked} title="Put into the active range" onClick={() => onUse(span)}>Use</Button>
              <Button variant="ghost" className="h-6 px-1.5 text-[11px]" disabled={picked} title="Add as another range" onClick={() => onAdd(span)}>+</Button>
            </div>
          );
        })}
        {indexed.length > SHOWN_INDEXED && (
          <p className="m-0 px-1.5 text-[11px] text-text-muted">and {formatCount(indexed.length - SHOWN_INDEXED)} older ranges</p>
        )}
      </div>
    </section>
  );
}
