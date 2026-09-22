import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';

import type { CoverageResponse, HistogramResponse } from '../api/types';
import { formatCount } from '../lib/format';
import { IconChevron } from './Icons';
import { Button, Field, FieldLabel, Input, Panel, RangeInput, cx } from './ui';

interface Props {
  from: number;
  to: number;
  coverage: CoverageResponse | null;
  histogram: HistogramResponse | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onFromChange: (value: string) => void;
  onToChange: (value: string) => void;
  onRangeChange: (next: Bounds) => void;
}

interface Bounds {
  from: number;
  to: number;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

function overlap(from: number, to: number, selection: Bounds): boolean {
  return from <= selection.to && to >= selection.from;
}

export function BlockRangeControl({
  from,
  to,
  coverage,
  histogram,
  open,
  onOpenChange,
  onFromChange,
  onToChange,
  onRangeChange,
}: Props) {
  const root = useRef<HTMLDivElement>(null);
  const [fromDraft, setFromDraft] = useState(String(from));
  const [toDraft, setToDraft] = useState(String(to));
  const [validation, setValidation] = useState<string | null>(null);

  useEffect(() => setFromDraft(String(from)), [from]);
  useEffect(() => setToDraft(String(to)), [to]);

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) onOpenChange(false);
    };

    document.addEventListener('pointerdown', onPointerDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
    };
  }, [onOpenChange, open]);

  const digitsOnly = (value: string) => value.replace(/\D/g, '');
  const selection = { from, to };
  const sliderBounds = useMemo<Bounds>(() => {
    // Keep room on both sides of the selected interval. Without a reachable
    // backend there is no stored span to use, so a ten-block selection must not
    // leave both native range handles pinned to opposite edges.
    const padding = Math.max(100, (to - from + 1) * 10);
    const storedFrom = coverage?.lowest_block ?? from - padding;
    const storedTo = coverage?.highest_block ?? to + padding;
    const head = coverage?.chain_head;
    return {
      from: Math.max(0, Math.min(from - padding, storedFrom, head === null || head === undefined ? from : head - 100)),
      to: Math.max(to + padding, storedTo, head ?? to),
    };
  }, [coverage, from, to]);

  const indexedInSelection = useMemo(() => {
    if (!coverage) return null;
    return coverage.ranges.reduce((total, range) => {
      const rangeFrom = Math.max(range.from_block, from);
      const rangeTo = Math.min(range.to_block, to);
      return total + (rangeTo >= rangeFrom ? rangeTo - rangeFrom + 1 : 0);
    }, 0);
  }, [coverage, from, to]);
  const selectedCount = to - from + 1;
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

  const commitInput = (key: 'from' | 'to', raw: string) => {
    const value = Number.parseInt(raw, 10);
    if (!Number.isFinite(value) || value < 0) {
      setValidation('Enter a non-negative block number.');
      return;
    }
    const next = { from, to, [key]: value };
    if (next.from > next.to) {
      setValidation('From block cannot be greater than To block.');
      return;
    }
    setValidation(null);
    if (key === 'from') onFromChange(String(value));
    else onToChange(String(value));
  };

  return (
    <div ref={root} className="relative flex-none">
      <Button
        variant="ghost"
        className={cx(
          'h-8 gap-1.5 px-2.5 text-xs font-medium text-text-muted',
          validation && 'text-critical',
        )}
        onClick={() => onOpenChange(!open)}
        aria-expanded={open}
        aria-controls="block-range-panel"
      >
        <span>Advanced</span>
        <IconChevron className={cx('transition-transform duration-150', open && 'rotate-180')} />
      </Button>

      {open && (
        <Panel
          id="block-range-panel"
          className="absolute left-0 top-[calc(100%+8px)] z-30 w-[360px] animate-drawer-in p-3 shadow-pop max-[620px]:left-auto max-[620px]:right-0 max-[420px]:w-[calc(100vw-24px)]"
          aria-label="Block range"
        >
          <div className="mb-3 flex items-center justify-between border-b border-hairline pb-2 text-[11px] text-text-muted">
            <span>Block range</span>
            <span className="tabular-nums text-text-secondary">
              {formatCount(from)} → {formatCount(to)}
            </span>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <Field>
              <FieldLabel htmlFor="q-from">From block</FieldLabel>
              <Input
                id="q-from"
                className="w-full tabular-nums"
                invalid={validation !== null}
                value={fromDraft}
                inputMode="numeric"
                onChange={(event) => {
                  const value = digitsOnly(event.target.value);
                  setFromDraft(value);
                  commitInput('from', value);
                }}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="q-to">To block</FieldLabel>
              <Input
                id="q-to"
                className="w-full tabular-nums"
                invalid={validation !== null}
                value={toDraft}
                inputMode="numeric"
                onChange={(event) => {
                  const value = digitsOnly(event.target.value);
                  setToDraft(value);
                  commitInput('to', value);
                }}
              />
            </Field>
          </div>

          {validation && <p className="mb-0 mt-2 text-[11px] text-critical">{validation}</p>}

          <section className="mt-3 border-t border-hairline pt-3" aria-label="Indexed coverage">
            <div className="mb-1.5 flex items-center justify-between text-[10px] uppercase tracking-[0.07em] text-text-muted">
              <span>Indexed coverage</span>
              {coverage?.persisted === false && <span>Storage off</span>}
            </div>
            <div className="relative flex h-10 items-end gap-px overflow-hidden rounded-ui-sm border border-hairline bg-plane px-1 py-1" title="Backend blocks available locally. The outlined interval is the selected range.">
              {histogram?.buckets.map((bucket) => {
                const indexed = bucket.indexed_blocks > 0;
                const selected = overlap(bucket.from_block, bucket.to_block, selection);
                const height = indexed && histogramPeak > 0
                  ? Math.max(18, (bucket.indexed_blocks / histogramPeak) * 100)
                  : 10;
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
              {!histogram && (
                <span className="grid h-full w-full place-items-center text-[11px] text-text-muted">Coverage unavailable</span>
              )}
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
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    setValidation(null);
                    onRangeChange({ from: value, to: Math.max(value, to) });
                  }}
                />
                <RangeInput
                  aria-label="To block"
                  className="pointer-events-none absolute inset-0 z-[3] h-6 appearance-none bg-transparent [&::-moz-range-thumb]:pointer-events-auto [&::-moz-range-track]:bg-transparent [&::-webkit-slider-runnable-track]:bg-transparent [&::-webkit-slider-thumb]:pointer-events-auto"
                  min={sliderBounds.from}
                  max={sliderBounds.to}
                  value={clamp(to, sliderBounds.from, sliderBounds.to)}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    setValidation(null);
                    onRangeChange({ from: Math.min(value, from), to: value });
                  }}
                />
              </div>
            </div>
          </section>

          <div className="mt-3 flex flex-wrap gap-x-3 gap-y-1 border-t border-hairline pt-2 text-[11px] text-text-muted">
            <span>Selected: <strong className="font-medium tabular-nums text-text-secondary">{formatCount(selectedCount)}</strong></span>
            {indexedInSelection !== null && <span>Indexed: <strong className="font-medium tabular-nums text-text-secondary">{formatCount(indexedInSelection)}</strong></span>}
            {coverage?.chain_head !== null && coverage?.chain_head !== undefined && <span>Head: <strong className="font-medium tabular-nums text-text-secondary">{formatCount(coverage.chain_head)}</strong></span>}
          </div>
        </Panel>
      )}
    </div>
  );
}
