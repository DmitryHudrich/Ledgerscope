import { useMemo, useState, type CSSProperties, type ReactNode } from 'react';

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
  collapsed: '40px',
  expanded: 'min(220px, 30vh)',
} as const;

type SortKey = 'block' | 'value' | 'asset';

interface Props {
  model: GraphModel;

  scope: string | null;
  linkScope: string | null;
  open: boolean;
  onOpenChange: (next: boolean) => void;
  onScopeClear: () => void;
  onSelect: (id: string) => void;
}

export function TxSheet({ model, scope, linkScope, open, onOpenChange, onScopeClear, onSelect }: Props) {
  const [sort, setSort] = useState<SortKey>('block');
  const [descending, setDescending] = useState(true);

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
      className="absolute bottom-0 left-0 right-0 z-12 flex h-[var(--sheet-height)] flex-col overflow-hidden border-t border-hairline bg-surface-1 shadow-panel transition-[height] duration-200 ease-[cubic-bezier(0.22,0.61,0.36,1)]"
      style={
        {
          '--sheet-height': open ? SHEET_HEIGHT.expanded : SHEET_HEIGHT.collapsed,
        } as CSSProperties
      }
      aria-label="Transaction table"
    >
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
