import { useMemo, useState, type CSSProperties } from 'react';

import type { CoverageResponse, HistogramResponse } from '../api/types';
import { clampRange, indexedWithin, sameRange, type BlockLimits, type BlockSpan, type RangeSource } from '../lib/blockRange';
import { formatCount } from '../lib/format';
import { IconClose } from './Icons';
import { BlockSpanText } from './BlockSpanText';
import { Button, Input, PanelSection, PanelTitle, cx } from './ui';

export type { BlockSpan };

interface Props {
  ranges: BlockSpan[];
  activeIndex: number;
  coverage: CoverageResponse | null;
  histogram: HistogramResponse | null;
  limits: BlockLimits;
  onActiveChange: (index: number) => void;
  onRangesChange: (ranges: BlockSpan[]) => void;
  onApply: () => void;
  onLatest: () => Promise<number | null>;
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
  onLatest,
  applyDisabled,
}: Props) {
  const [latestFor, setLatestFor] = useState<number | null>(null);
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
    onRangesChange([...ranges, { ...active }]);
    onActiveChange(ranges.length);
  };

  const toLatest = async (index: number) => {
    setLatestFor(index);
    try {
      const head = await onLatest();
      const range = ranges[index];
      if (head === null || !range) return;
      const moved = clampRange({ ...range, to: head }, { ...limits, head }, 'to');
      onRangesChange(ranges.map((item, idx) => (idx === index ? moved : item)));
    } finally {
      setLatestFor(null);
    }
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
          const inverted = range.from > range.to;
          return (
            <div
              key={index}
              title={inverted ? 'The start is above the end: the range is read backwards, from the smaller block to the bigger one' : undefined}
              className={cx(
                'flex items-center gap-1.5 rounded-ui-sm px-1.5 py-1',
                inverted && 'outline outline-1 outline-critical',
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
                className="h-6 flex-none px-1.5 text-[10px] uppercase tracking-wide"
                title="Set the end to the newest block the nodes report now"
                disabled={latestFor !== null}
                onClick={() => void toLatest(index)}
              >
                {latestFor === index ? '…' : 'latest'}
              </Button>
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

      <RangeSettings
        range={active}
        coverage={coverage}
        onChange={(patch) => patchRange(activeIndex, patch)}
      />

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
                <BlockSpanText from={range.from_block} to={range.to_block} />
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

function covered(ranges: Array<{ from_block: number; to_block: number }>, span: BlockSpan): number {
  return ranges.reduce((sum, range) => {
    const low = Math.max(range.from_block, span.from);
    const high = Math.min(range.to_block, span.to);
    return sum + (high >= low ? high - low + 1 : 0);
  }, 0);
}

const SOURCE_TITLE: Record<RangeSource, string> = {
  auto: 'Auto',
  rpc: 'RPC',
  dataset: 'AWS dataset',
};

function RangeSettings({ range, coverage, onChange }: {
  range: BlockSpan;
  coverage: CoverageResponse | null;
  onChange: (patch: Partial<BlockSpan>) => void;
}) {
  const total = range.to - range.from + 1;
  const indexed = covered(coverage?.ranges ?? [], range);
  const approximate = covered(coverage?.approximate ?? [], range);
  const exact = indexed - approximate;
  const missing = Math.max(total - indexed, 0);
  const dataset = coverage?.dataset ?? null;
  const reach = dataset?.available_until ?? null;
  const fromDataset = reach === null ? 0 : covered([{ from_block: 0, to_block: reach }], range);
  const source = range.source ?? 'auto';

  const share = (count: number) => `${total > 0 ? Math.round((count / total) * 100) : 0}%`;

  return (
    <section className="mt-3 grid gap-2 border-t border-hairline pt-3" aria-label="Active range settings">
      <div className="text-[10px] uppercase tracking-[0.07em] text-text-muted">Active range scan</div>
      <div className="flex items-center gap-2 text-xs text-text-secondary">
        <span className="w-14 flex-none">Source</span>
        <div className="flex flex-1 rounded-ui-sm border border-hairline p-0.5" role="radiogroup" aria-label="Where missing blocks come from">
          {(['auto', 'rpc', 'dataset'] as const).map((value) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={source === value}
              disabled={value === 'dataset' && !dataset}
              title={
                value === 'auto'
                  ? dataset ? 'The dataset for blocks it already has, the RPC for newer ones' : 'The RPC, no dataset is configured'
                  : value === 'rpc'
                    ? 'Every missing block from the RPC nodes, exact values'
                    : dataset ? 'The AWS public dataset, values in wei are approximate' : 'No dataset is configured on the server'
              }
              className={cx(
                'h-6 flex-1 cursor-pointer rounded-[4px] border-0 bg-transparent px-1 text-[11px] disabled:cursor-default disabled:opacity-40',
                source === value ? 'bg-[color-mix(in_srgb,var(--accent)_14%,transparent)] text-accent' : 'text-text-secondary',
              )}
              onClick={() => onChange({ source: value })}
            >
              {SOURCE_TITLE[value]}
            </button>
          ))}
        </div>
      </div>

      <dl className="m-0 grid grid-cols-[1fr_auto_auto] gap-x-2 gap-y-0.5 text-[11px]">
        <dt className="text-text-secondary">Exact, from RPC</dt>
        <dd className="m-0 text-right tabular-nums">{formatCount(exact)}</dd>
        <dd className="m-0 w-9 text-right tabular-nums text-text-muted">{share(exact)}</dd>
        <dt className="text-text-secondary">Approximate, from dataset</dt>
        <dd className={cx('m-0 text-right tabular-nums', approximate > 0 && 'text-series-focus')}>{formatCount(approximate)}</dd>
        <dd className="m-0 w-9 text-right tabular-nums text-text-muted">{share(approximate)}</dd>
        <dt className="text-text-secondary">Not indexed</dt>
        <dd className="m-0 text-right tabular-nums">{formatCount(missing)}</dd>
        <dd className="m-0 w-9 text-right tabular-nums text-text-muted">{share(missing)}</dd>
      </dl>

      {dataset && (
        <p className="m-0 text-[11px] leading-relaxed text-text-muted">
          The dataset reaches block {reach === null ? 'unknown' : formatCount(reach)}
          {fromDataset > 0 && fromDataset < total ? `, ${formatCount(fromDataset)} of these blocks` : fromDataset >= total ? ', the whole range' : ', none of this range'}.
          Its ETH amounts are doubles, so the last digits in wei can be off.
        </p>
      )}

      <label className="flex cursor-pointer items-start gap-2 text-xs text-text-secondary">
        <input
          type="checkbox"
          className="mt-0.5 accent-accent"
          checked={range.exact ?? false}
          onChange={(event) => onChange({ exact: event.target.checked })}
        />
        <span>
          Exact wei: fetch dataset blocks of this range again over RPC
          {approximate > 0 && <span className="text-text-muted"> ({formatCount(approximate)} blocks on Apply)</span>}
        </span>
      </label>
    </section>
  );
}
