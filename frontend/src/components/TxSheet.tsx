import { useMemo, useState } from 'react';

import type { TxResponse } from '../api/types';
import type { GraphModel } from '../graph/model';
import {
  formatCount,
  formatEth,
  formatRelative,
  formatTimestamp,
  methodId,
  parseWei,
  shortAddress,
  shortHash,
} from '../lib/format';
import { IconChevron } from './Icons';

const ROW_LIMIT = 500;

/** Shared with the stage so the overlays can keep clear of the sheet. */
export const SHEET_HEIGHT = { collapsed: 40, expanded: 320 } as const;

type SortKey = 'block' | 'value';

interface Props {
  model: GraphModel;
  /** Address the table is scoped to, or null for the whole graph. */
  scope: string | null;
  open: boolean;
  onOpenChange: (next: boolean) => void;
  onScopeClear: () => void;
  onSelect: (id: string) => void;
}

export function TxSheet({ model, scope, open, onOpenChange, onScopeClear, onSelect }: Props) {
  const [sort, setSort] = useState<SortKey>('block');
  const [descending, setDescending] = useState(true);

  const rows = useMemo(() => {
    const links = scope ? (model.linksByNode.get(scope) ?? []) : model.links;
    // Wei is parsed once per row, not once per comparison.
    const all: Array<{ tx: TxResponse; wei: bigint }> = [];
    for (const link of links) {
      for (const tx of link.txs) all.push({ tx, wei: parseWei(tx.amount) });
    }
    all.sort((a, b) => {
      let delta: number;
      if (sort === 'block') delta = a.tx.block_number - b.tx.block_number;
      else delta = a.wei === b.wei ? 0 : a.wei < b.wei ? -1 : 1;
      return descending ? -delta : delta;
    });
    return all;
  }, [model, scope, sort, descending]);

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
      className="overlay sheet"
      style={{ height: open ? SHEET_HEIGHT.expanded : SHEET_HEIGHT.collapsed }}
      aria-label="Transaction table"
    >
      <div className="sheet-head">
        <button
          type="button"
          className="btn btn-ghost sheet-toggle"
          onClick={() => onOpenChange(!open)}
          aria-expanded={open}
        >
          <IconChevron
            style={{
              transform: open ? 'rotate(180deg)' : 'none',
              transition: 'transform 180ms ease',
            }}
          />
          <span className="sheet-title">Transactions</span>
          <span className="badge">{formatCount(rows.length)}</span>
        </button>
        {scope && (
          <>
            <span className="tag">scoped to {shortAddress(scope, 8, 6)}</span>
            <button type="button" className="btn btn-ghost" onClick={onScopeClear}>
              Show all
            </button>
          </>
        )}
      </div>

      {open && (
        <div className="sheet-body">
          <table className="table">
            <thead>
              <tr>
                <th className="num">
                  <button type="button" onClick={() => toggleSort('block')}>
                    Block{arrow('block')}
                  </button>
                </th>
                <th>Age</th>
                <th>From</th>
                <th>To</th>
                <th className="num">
                  <button type="button" onClick={() => toggleSort('value')}>
                    Value (ETH){arrow('value')}
                  </button>
                </th>
                <th>Type</th>
                <th>Tx hash</th>
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, ROW_LIMIT).map(({ tx, wei }) => {
                const method = methodId(tx.data);
                return (
                  <tr key={tx.tx_hash}>
                    <td className="num">{formatCount(tx.block_number)}</td>
                    <td title={formatTimestamp(tx.timestamp)}>{formatRelative(tx.timestamp)}</td>
                    <td className="mono">
                      <button
                        type="button"
                        className="cell-link"
                        onClick={() => onSelect(tx.from.toLowerCase())}
                      >
                        {shortAddress(tx.from, 8, 6)}
                      </button>
                    </td>
                    <td className="mono">
                      {tx.to ? (
                        <button
                          type="button"
                          className="cell-link"
                          onClick={() => onSelect(tx.to!.toLowerCase())}
                        >
                          {shortAddress(tx.to, 8, 6)}
                        </button>
                      ) : (
                        <span className="tag">contract creation</span>
                      )}
                    </td>
                    <td className="num">{formatEth(wei)}</td>
                    <td>
                      <span className="tag">
                        {method ? (
                          <>
                            call <code className="mono">{method}</code>
                          </>
                        ) : (
                          'transfer'
                        )}
                      </span>
                    </td>
                    <td className="mono">{shortHash(tx.tx_hash)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {rows.length > ROW_LIMIT && (
            <p className="legend-note" style={{ padding: '10px 12px' }}>
              Showing the first {formatCount(ROW_LIMIT)} of {formatCount(rows.length)} rows — narrow
              the range or raise the minimum-flow filter to see the rest.
            </p>
          )}
          {rows.length === 0 && (
            <p className="legend-note" style={{ padding: '14px 12px' }}>
              No transactions match the current filters.
            </p>
          )}
        </div>
      )}
    </section>
  );
}
