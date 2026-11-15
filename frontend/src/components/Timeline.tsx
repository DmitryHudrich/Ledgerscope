import { useMemo } from 'react';

import type { CoverageResponse, HistogramResponse } from '../api/types';
import { formatCompact, formatCount } from '../lib/format';

export interface BlockBounds {
  from: number;
  to: number;
}

interface Props {
  bounds: BlockBounds;
  selection: BlockBounds;
  coverage: CoverageResponse | null;
  histogram: HistogramResponse | null;
  maxBlocks: number;
  onBoundsChange: (next: BlockBounds) => void;
  onSelectionChange: (next: BlockBounds) => void;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

function storeLabel(coverage: CoverageResponse | null): string {
  if (!coverage) return 'store: backend unreachable';
  if (!coverage.persisted) return 'store: off (storage.persist_txs)';
  if (coverage.block_count === 0) return 'store: empty, every block needs the node';

  return `store: ${formatCount(coverage.block_count)} blocks · ${formatCompact(
    coverage.tx_count,
  )} txs`;
}

function storeTone(coverage: CoverageResponse | null): string | undefined {
  return !coverage || !coverage.persisted || coverage.block_count === 0
    ? 'timeline-warn'
    : undefined;
}

export function Timeline({
  bounds,
  selection,
  coverage,
  histogram,
  maxBlocks,
  onBoundsChange,
  onSelectionChange,
}: Props) {
  const span = Math.max(bounds.to - bounds.from, 1);
  const peak = useMemo(
    () => histogram?.buckets.reduce((top, bucket) => Math.max(top, bucket.tx_count), 0) ?? 0,
    [histogram],
  );

  const indexedInWindow = useMemo(() => {
    if (!coverage) return 0;
    return coverage.ranges.reduce((sum, range) => {
      const from = Math.max(range.from_block, selection.from);
      const to = Math.min(range.to_block, selection.to);
      return sum + (to >= from ? to - from + 1 : 0);
    }, 0);
  }, [coverage, selection]);

  const picked = selection.to - selection.from + 1;
  const missing = Math.max(picked - indexedInWindow, 0);
  const tooWide = picked > maxBlocks;

  const left = ((selection.from - bounds.from) / span) * 100;
  const width = ((selection.to - selection.from) / span) * 100;

  const editBound = (key: keyof BlockBounds, raw: string) => {
    const value = Number.parseInt(raw, 10);
    if (!Number.isFinite(value) || value < 0) return;

    const next = { ...bounds, [key]: value };
    if (next.from > next.to) return;
    onBoundsChange(next);
  };

  return (
    <div className="timeline panel overlay">
      <div className="timeline-head">
        <div className="field">
          <label htmlFor="t-low">Slider from block</label>
          <input
            id="t-low"
            className="input input-block"
            value={String(bounds.from)}
            inputMode="numeric"
            onChange={(event) => editBound('from', event.target.value.replace(/\D/g, ''))}
          />
        </div>
        <div className="field">
          <label htmlFor="t-high">Slider to block</label>
          <input
            id="t-high"
            className="input input-block"
            value={String(bounds.to)}
            inputMode="numeric"
            onChange={(event) => editBound('to', event.target.value.replace(/\D/g, ''))}
          />
        </div>

        <div className="timeline-facts">
          <span>
            <strong>{formatCount(picked)}</strong> blocks picked
          </span>
          <span className={missing > 0 ? 'timeline-warn' : undefined}>
            <strong>{formatCount(indexedInWindow)}</strong> in clickhouse
            {missing > 0 && <> · {formatCount(missing)} need the node</>}
          </span>
          <span className={storeTone(coverage)}>{storeLabel(coverage)}</span>
          {coverage?.chain_head != null && (
            <span>head {formatCount(coverage.chain_head)}</span>
          )}
          {tooWide && (
            <span className="timeline-warn">over the {formatCount(maxBlocks)} block limit</span>
          )}
        </div>
      </div>

      <div className="timeline-track">
        <div className="timeline-bars" aria-hidden="true">
          {histogram?.buckets.map((bucket) => (
            <div
              className={`timeline-bar${bucket.indexed_blocks > 0 ? ' timeline-bar-indexed' : ''}`}
              key={bucket.from_block}
              style={{ height: `${peak > 0 ? (bucket.tx_count / peak) * 100 : 0}%` }}
              title={`${formatCount(bucket.from_block)}–${formatCount(bucket.to_block)}: ${formatCount(
                bucket.tx_count,
              )} txs, ${formatCount(bucket.indexed_blocks)} blocks indexed`}
            />
          ))}
        </div>
        <div
          className="timeline-window"
          style={{ left: `${clamp(left, 0, 100)}%`, width: `${clamp(width, 0, 100)}%` }}
          aria-hidden="true"
        />
      </div>

      <div className="timeline-sliders">
        <label className="timeline-slider">
          <span>From {formatCount(selection.from)}</span>
          <input
            type="range"
            min={bounds.from}
            max={bounds.to}
            value={clamp(selection.from, bounds.from, bounds.to)}
            onChange={(event) => {
              const from = Number(event.target.value);
              onSelectionChange({ from, to: Math.max(from, selection.to) });
            }}
          />
        </label>
        <label className="timeline-slider">
          <span>To {formatCount(selection.to)}</span>
          <input
            type="range"
            min={bounds.from}
            max={bounds.to}
            value={clamp(selection.to, bounds.from, bounds.to)}
            onChange={(event) => {
              const to = Number(event.target.value);
              onSelectionChange({ from: Math.min(to, selection.from), to });
            }}
          />
        </label>
      </div>
    </div>
  );
}
