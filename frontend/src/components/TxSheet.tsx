import { useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type ReactNode } from 'react';

import { edgeEndpoints, edgeLabel, edgeTransfer, type EdgeTransfer } from '../api/edges';
import type { GraphEdge } from '../api/types';
import type { GraphModel } from '../graph/model';
import {
  formatCount,
  formatRelative,
  formatTimestamp,
  formatUnits,
  shortAddress,
  shortHash,
  unitsToNumber,
} from '../lib/format';
import { IconChevron } from './Icons';
import { Badge, Button, Dot, Tag, cx, focusRing } from './ui';

const ROW_LIMIT = 500;

export const SHEET_HEIGHT = {
  collapsed: 40,
  default: 220,
  min: 140,
  topReserve: 160,
} as const;

type SortKey = 'block' | 'value' | 'asset';

interface Props {
  model: GraphModel;

  scope: string | null;
  linkScope: string | null;
  open: boolean;
  height: number;
  onOpenChange: (next: boolean) => void;
  onHeightChange: (next: number) => void;
  onScopeClear: () => void;
  onSelect: (id: string) => void;
}

export function TxSheet({ model, scope, linkScope, open, height, onOpenChange, onHeightChange, onScopeClear, onSelect }: Props) {
  const [sort, setSort] = useState<SortKey>('block');
  const [descending, setDescending] = useState(true);
  const [resizing, setResizing] = useState(false);
  const sheetRef = useRef<HTMLElement>(null);

  const bounds = () => {
    const available = sheetRef.current?.parentElement?.clientHeight ?? window.innerHeight;
    return {
      min: Math.min(SHEET_HEIGHT.min, Math.max(SHEET_HEIGHT.collapsed, available - SHEET_HEIGHT.topReserve)),
      max: Math.max(SHEET_HEIGHT.collapsed, available - SHEET_HEIGHT.topReserve),
    };
  };
  const clampHeight = (next: number) => {
    const { min, max } = bounds();
    return Math.round(Math.min(max, Math.max(min, next)));
  };

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    if (!open) onOpenChange(true);
    const handle = event.currentTarget;
    const startY = event.clientY;
    const startHeight = open ? height : SHEET_HEIGHT.default;
    setResizing(true);
    handle.setPointerCapture(event.pointerId);

    const move = (moveEvent: globalThis.PointerEvent) => {
      onHeightChange(clampHeight(startHeight + startY - moveEvent.clientY));
    };
    const finish = () => {
      setResizing(false);
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', finish);
      handle.removeEventListener('pointercancel', finish);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', finish);
    handle.addEventListener('pointercancel', finish);
  };

  const resizeWithKeyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    let next: number | null = null;
    if (event.key === 'ArrowUp') next = height + 20;
    if (event.key === 'ArrowDown') next = height - 20;
    if (event.key === 'Home') next = bounds().min;
    if (event.key === 'End') next = bounds().max;
    if (next === null) return;
    event.preventDefault();
    if (!open) onOpenChange(true);
    onHeightChange(clampHeight(next));
  };

  const rows = useMemo(() => {
    const links = linkScope ? model.links.filter((link) => link.id === linkScope) : scope ? (model.linksByNode.get(scope) ?? []) : model.links;

    const all: Array<{ tx: GraphEdge; transfer: EdgeTransfer | null; magnitude: number }> = [];
    for (const link of links) {
      for (const tx of link.txs) {
        const transfer = edgeTransfer(tx);
        all.push({
          tx,
          transfer,
          magnitude: transfer ? unitsToNumber(transfer.amount, transfer.decimals) : -1,
        });
      }
    }
    all.sort((a, b) => {
      let delta: number;
      if (sort === 'block') delta = a.tx.block_number - b.tx.block_number;
      else if (sort === 'asset') {
        const left = a.transfer?.symbol ?? '';
        const right = b.transfer?.symbol ?? '';
        delta = left.localeCompare(right);
      } else delta = a.magnitude - b.magnitude;
      return descending ? -delta : delta;
    });
    return all;
  }, [linkScope, model, scope, sort, descending]);

  const toggleSort = (key: SortKey) => {
    if (key === sort) setDescending((value) => !value);
    else {
      setSort(key);
      setDescending(true);
    }
  };

  const arrow = (key: SortKey) => (key === sort ? (descending ? ' ↓' : ' ↑') : '');

  return (
    <section
      ref={sheetRef}
      className={cx(
        'absolute bottom-0 left-0 right-0 z-12 flex h-[var(--sheet-height)] flex-col overflow-hidden border-t border-hairline bg-surface-1 shadow-panel',
        resizing && 'select-none',
      )}
      style={
        {
          '--sheet-height': `${open ? height : SHEET_HEIGHT.collapsed}px`,
        } as CSSProperties
      }
      aria-label="Transaction table"
    >
      {open && (
        <div
          role="separator"
          aria-label="Resize transactions panel"
          aria-orientation="horizontal"
          aria-valuemin={bounds().min}
          aria-valuemax={bounds().max}
          aria-valuenow={height}
          tabIndex={0}
          className={cx(
            'group absolute inset-x-0 top-0 z-20 h-2 cursor-ns-resize touch-none outline-none',
            'focus-visible:bg-[color-mix(in_srgb,var(--accent)_10%,transparent)]',
          )}
          onPointerDown={startResize}
          onKeyDown={resizeWithKeyboard}
        >
          <span className="absolute left-1/2 top-0.5 h-1 w-10 -translate-x-1/2 rounded-full bg-hairline transition-colors group-hover:bg-accent group-focus-visible:bg-accent" aria-hidden="true" />
        </div>
      )}
      <div className="flex h-10 flex-none items-center gap-2 px-[10px]">
        <Button
          variant="ghost"
          className="h-[30px] gap-[9px]"
          onClick={() => onOpenChange(!open)}
          aria-expanded={open}
        >
          <IconChevron
            className={cx('transition-transform duration-180', !open && 'rotate-180')}
          />
          <span className="text-xs font-semibold uppercase tracking-[0.04em] text-text-secondary">
            Transactions
          </span>
          <Badge>{formatCount(rows.length)}</Badge>
        </Button>
        {(scope || linkScope) && (
          <>
            <Tag>{linkScope ? 'scoped to selected transfer' : `scoped to ${shortAddress(scope!, 8, 6)}`}</Tag>
            <Button variant="ghost" onClick={onScopeClear}>
              Show all
            </Button>
          </>
        )}
      </div>

      {open && (
        <div className="overflow-auto overscroll-contain">
          <table className="w-full border-collapse text-[12.5px] [&_td]:whitespace-nowrap [&_td]:border-b [&_td]:border-hairline [&_td]:px-3 [&_td]:py-[7px] [&_th]:sticky [&_th]:top-0 [&_th]:z-1 [&_th]:whitespace-nowrap [&_th]:border-b [&_th]:border-hairline [&_th]:bg-surface-1 [&_th]:px-3 [&_th]:py-2 [&_th]:text-left [&_th]:text-[10px] [&_th]:font-semibold [&_th]:uppercase [&_th]:tracking-[0.07em] [&_th]:text-text-muted [&_tbody_tr:hover_td]:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]">
            <thead>
              <tr>
                <th className="num">
                  <SortButton onClick={() => toggleSort('block')}>
                    Block{arrow('block')}
                  </SortButton>
                </th>
                <th>Age</th>
                <th>From</th>
                <th>To</th>
                <th className="num">
                  <SortButton onClick={() => toggleSort('value')}>
                    Amount{arrow('value')}
                  </SortButton>
                </th>
                <th>
                  <SortButton onClick={() => toggleSort('asset')}>
                    Asset{arrow('asset')}
                  </SortButton>
                </th>
                <th>Type</th>
                <th>Tx hash</th>
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, ROW_LIMIT).map(({ tx, transfer }, index) => {
                const endpoints = edgeEndpoints(tx);
                return (
                  <tr
                    key={`${tx.tx_hash}:${index}`}
                    className={tx.succeeded ? undefined : 'text-text-muted'}
                  >
                    <td className="text-right tabular-nums">{formatCount(tx.block_number)}</td>
                    <td title={formatTimestamp(tx.timestamp)}>{formatRelative(tx.timestamp)}</td>
                    <td className="font-mono-ui text-[11.5px]">
                      {endpoints ? (
                        <CellLink
                          failed={!tx.succeeded}
                          onClick={() => onSelect(endpoints.from.toLowerCase())}
                        >
                          {shortAddress(endpoints.from, 8, 6)}
                        </CellLink>
                      ) : (
                        '—'
                      )}
                    </td>
                    <td className="font-mono-ui text-[11.5px]">
                      {endpoints ? (
                        <CellLink
                          failed={!tx.succeeded}
                          onClick={() => onSelect(endpoints.to.toLowerCase())}
                        >
                          {shortAddress(endpoints.to, 8, 6)}
                        </CellLink>
                      ) : (
                        '—'
                      )}
                    </td>
                    <td className="text-right tabular-nums">
                      {transfer ? formatUnits(transfer.amount, transfer.decimals) : '—'}
                    </td>
                    <td>
                      {transfer ? (
                        <Tag title={transfer.native ? 'ETH' : transfer.key}>
                          <Dot tone={transfer.native ? 'focus' : 'token'} />
                          <code className="font-mono-ui">{transfer.symbol}</code>
                        </Tag>
                      ) : (
                        '—'
                      )}
                    </td>
                    <td>
                      <Tag>
                        {edgeLabel(tx)}
                        {!tx.succeeded && (
                          <span className="text-[10px] uppercase tracking-[0.04em] text-critical">
                            reverted
                          </span>
                        )}
                      </Tag>
                    </td>
                    <td className="font-mono-ui text-[11.5px]">{shortHash(tx.tx_hash)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {rows.length > ROW_LIMIT && (
            <p className="m-0 px-3 py-[10px] text-[11px] text-text-muted">
              Showing the first {formatCount(ROW_LIMIT)} of {formatCount(rows.length)} rows — narrow
              the range or raise the minimum-flow filter to see the rest.
            </p>
          )}
          {rows.length === 0 && (
            <p className="m-0 px-3 py-[14px] text-[11px] text-text-muted">
              No transactions match the current filters.
            </p>
          )}
        </div>
      )}
    </section>
  );
}

function SortButton({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return (
    <button
      type="button"
      className={cx(
        'cursor-pointer border-0 bg-transparent p-0 font-inherit uppercase tracking-inherit text-inherit',
        focusRing,
      )}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function CellLink({
  children,
  onClick,
  failed,
}: {
  children: ReactNode;
  onClick: () => void;
  failed: boolean;
}) {
  return (
    <button
      type="button"
      className={cx(
        'cursor-pointer border-0 bg-transparent p-0 font-inherit text-accent hover:underline',
        focusRing,
        failed && 'line-through',
      )}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
