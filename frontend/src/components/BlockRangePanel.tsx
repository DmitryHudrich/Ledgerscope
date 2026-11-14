import { useMemo, type CSSProperties } from 'react';

import type { CoverageResponse, HistogramResponse } from '../api/types';
import { clampRange, type BlockLimits, type RangeEdge } from '../lib/blockRange';
import { formatCount } from '../lib/format';
import { IconClose } from './Icons';
import { Button, Input, PanelSection, PanelTitle, RangeInput, cx } from './ui';

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
  limits,
  onActiveChange,
  onRangesChange,
  onApply,
  applyDisabled,
}: Props) {
  const active = ranges[activeIndex] ?? ranges[0] ?? { from: 0, to: 0 };
  const { from, to } = active;

  const sliderBounds = useMemo(() => {
    const padding = Math.max(100, (to - from + 1) * 10);
    const storedFrom = coverage?.lowest_block ?? from - padding;
    const storedTo = coverage?.highest_block ?? to + padding;
    const head = limits.head;
    const low = Math.max(0, Math.min(from - padding, storedFrom, head === null ? from : head - 100));
    const high = Math.max(to + padding, storedTo, head ?? to);
    return { from: low, to: high };
  }, [coverage, from, limits.head, to]);

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
  const sliderWindow = useMemo(() => {
    const span = Math.max(sliderBounds.to - sliderBounds.from, 1);
    return {
      left: ((from - sliderBounds.from) / span) * 100,
      right: ((to - sliderBounds.from) / span) * 100,
    };
  }, [from, sliderBounds, to]);

  const patchRange = (index: number, patch: Partial<BlockSpan>) =>
    onRangesChange(ranges.map((item, idx) => (idx === index ? { ...item, ...patch } : item)));

  const slide = (key: RangeEdge, value: number) =>
    onRangesChange(
      ranges.map((item, idx) =>
        idx === activeIndex
          ? clampRange(key === 'from' ? { from: value, to } : { from, to: value }, limits, key)
          : item,
      ),
    );

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
        <div className="mt-2 grid gap-1">
          <div className="flex justify-between text-[11px] text-text-secondary">
            <span>From <span className="tabular-nums">{formatCount(from)}</span></span>
            <span>To <span className="tabular-nums">{formatCount(to)}</span></span>
          </div>
          <div className="relative h-6">
            <div className="absolute left-0 right-0 top-1/2 h-1.5 -translate-y-1/2 rounded-full bg-hairline-strong" aria-hidden="true" />
            <div
              className="pointer-events-none absolute top-1/2 h-1.5 -translate-y-1/2 rounded-full bg-accent"
              style={{ left: `${sliderWindow.left}%`, width: `${Math.max(sliderWindow.right - sliderWindow.left, 0)}%` }}
              aria-hidden="true"
            />
            <RangeInput
              aria-label="From block"
              className="pointer-events-none absolute inset-0 z-[2] h-6 appearance-none bg-transparent [&::-moz-range-thumb]:pointer-events-auto [&::-moz-range-track]:bg-transparent [&::-webkit-slider-runnable-track]:bg-transparent [&::-webkit-slider-thumb]:pointer-events-auto"
              min={sliderBounds.from}
              max={sliderBounds.to}
              value={clamp(from, sliderBounds.from, sliderBounds.to)}
              onChange={(event) => slide('from', Number(event.target.value))}
            />
            <RangeInput
              aria-label="To block"
              className="pointer-events-none absolute inset-0 z-[3] h-6 appearance-none bg-transparent [&::-moz-range-thumb]:pointer-events-auto [&::-moz-range-track]:bg-transparent [&::-webkit-slider-runnable-track]:bg-transparent [&::-webkit-slider-thumb]:pointer-events-auto"
              min={sliderBounds.from}
              max={sliderBounds.to}
              value={clamp(to, sliderBounds.from, sliderBounds.to)}
              onChange={(event) => slide('to', Number(event.target.value))}
            />
          </div>
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
