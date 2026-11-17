import { useMemo, useState } from 'react';
import { BlockSpanText } from './BlockSpanText';

import type { GraphEdge } from '../api/types';
import { displayAddress } from '../lib/chain';
import { formatCount, formatTimestamp, formatUnits, shortAddress } from '../lib/format';
import { GROUP_COLORS, groupStats, type GraphGroup } from '../lib/groups';
import { IconChevron, IconEye, IconEyeOff, IconTarget } from './Icons';
import { Badge, Button, PanelSection, PanelTitle, cx, focusRing } from './ui';

interface Props {
  groups: GraphGroup[];
  edges: GraphEdge[];
  selection: string[];
  hiddenNodes: Set<string>;
  hiddenTxs: Set<string>;
  onGroupsChange: (next: GraphGroup[]) => void;
  onCreateFromSelection: (name: string) => void;
  onSelectMembers: (ids: string[]) => void;
  onShowNode: (id: string) => void;
  onShowAllNodes: () => void;
  onShowAllTxs: () => void;
}

const HIDDEN_PREVIEW = 8;

export function GroupsPanel({
  groups,
  edges,
  selection,
  hiddenNodes,
  hiddenTxs,
  onGroupsChange,
  onCreateFromSelection,
  onSelectMembers,
  onShowNode,
  onShowAllNodes,
  onShowAllTxs,
}: Props) {
  const [open, setOpen] = useState<string | null>(groups[0]?.id ?? null);
  const [name, setName] = useState('');
  const [renaming, setRenaming] = useState<string | null>(null);
  const [showAllHidden, setShowAllHidden] = useState(false);

  const stats = useMemo(() => new Map(groups.map((group) => [group.id, groupStats(group, edges)])), [edges, groups]);

  const patch = (id: string, change: Partial<GraphGroup>) =>
    onGroupsChange(groups.map((group) => (group.id === id ? { ...group, ...change } : group)));

  const hiddenList = [...hiddenNodes];

  return (
    <>
      <PanelSection>
        <PanelTitle>New group</PanelTitle>
        <form
          className="flex gap-1.5"
          onSubmit={(event) => {
            event.preventDefault();
            onCreateFromSelection(name);
            setName('');
          }}
        >
          <input
            className={cx('h-8 min-w-0 flex-1 rounded-ui-sm border border-hairline bg-plane px-2 text-xs', focusRing)}
            placeholder={`Group ${groups.length + 1}`}
            maxLength={60}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <Button type="submit" className="h-8 px-2.5 text-xs" disabled={selection.length === 0}>
            From {selection.length || 'no'} selected
          </Button>
        </form>
        <p className="mb-0 mt-2 text-[11px] leading-relaxed text-text-muted">
          Select nodes with Shift-click or a drag box. Transactions join a group from the transfer panel or the transaction table.
        </p>
      </PanelSection>

      {groups.length === 0 && (
        <PanelSection>
          <p className="m-0 text-xs text-text-muted">No groups yet.</p>
        </PanelSection>
      )}

      {groups.map((group) => {
        const numbers = stats.get(group.id);
        const expanded = open === group.id;
        return (
          <PanelSection key={group.id} className="py-2.5">
            <div className="flex items-center gap-1.5">
              <label className="relative size-3.5 flex-none cursor-pointer rounded-full" style={{ background: group.color }} title="Colour">
                <select
                  className="absolute inset-0 cursor-pointer opacity-0"
                  value={group.color}
                  aria-label={`Colour of ${group.name}`}
                  onChange={(event) => patch(group.id, { color: event.target.value })}
                >
                  {GROUP_COLORS.map((color) => <option key={color} value={color}>{color}</option>)}
                </select>
              </label>
              {renaming === group.id ? (
                <input
                  className={cx('h-7 min-w-0 flex-1 rounded-ui-sm border border-hairline bg-plane px-1.5 text-xs', focusRing)}
                  defaultValue={group.name}
                  autoFocus
                  maxLength={60}
                  onBlur={(event) => {
                    patch(group.id, { name: event.target.value.trim() || group.name });
                    setRenaming(null);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') event.currentTarget.blur();
                    if (event.key === 'Escape') setRenaming(null);
                  }}
                />
              ) : (
                <button
                  type="button"
                  className={cx('flex min-w-0 flex-1 cursor-pointer items-center gap-1 text-left text-[13px] font-medium', group.hidden && 'text-text-muted line-through', focusRing)}
                  onClick={() => setOpen(expanded ? null : group.id)}
                  onDoubleClick={() => setRenaming(group.id)}
                  title="Click for stats, double-click to rename"
                >
                  <IconChevron size={12} className={cx('flex-none text-text-muted transition-transform', !expanded && '-rotate-90')} />
                  <span className="truncate">{group.name}</span>
                </button>
              )}
              <Badge title="Nodes · transactions">{group.nodes.length}·{group.txs.length}</Badge>
              <Button
                variant="ghost"
                icon
                className="size-7"
                aria-label={group.hidden ? `Show ${group.name}` : `Hide ${group.name}`}
                title={group.hidden ? 'Show on canvas' : 'Hide from canvas'}
                onClick={() => patch(group.id, { hidden: !group.hidden })}
              >
                {group.hidden ? <IconEyeOff size={14} /> : <IconEye size={14} />}
              </Button>
            </div>

            {expanded && numbers && (
              <div className="mt-2 grid gap-2 text-[11px]">
                <div className="grid grid-cols-2 gap-px overflow-hidden rounded-ui-sm border border-hairline bg-hairline">
                  {[
                    ['Nodes', numbers.nodes],
                    ['Txs', numbers.txs],
                    ['Failed', numbers.failed],
                    ['Counterparties', numbers.counterparties],
                  ].map(([title, value]) => (
                    <div key={title} className="bg-surface-1 px-2 py-1.5">
                      <div className="text-[9px] uppercase tracking-[0.06em] text-text-muted">{title}</div>
                      <div className="text-[13px] font-semibold tabular-nums">{formatCount(Number(value))}</div>
                    </div>
                  ))}
                </div>
                {numbers.firstBlock !== null && (
                  <div className="text-text-secondary">
                    Blocks <BlockSpanText from={numbers.firstBlock} to={numbers.lastBlock ?? numbers.firstBlock} />
                    {numbers.firstTime !== null && (
                      <div className="text-text-muted">{formatTimestamp(numbers.firstTime)} → {formatTimestamp(numbers.lastTime ?? numbers.firstTime)}</div>
                    )}
                  </div>
                )}
                {numbers.flows.length > 0 && (
                  <table className="w-full border-collapse tabular-nums [&_td]:py-0.5 [&_th]:pb-1 [&_th]:text-left [&_th]:text-[9px] [&_th]:font-semibold [&_th]:uppercase [&_th]:tracking-[0.06em] [&_th]:text-text-muted">
                    <thead>
                      <tr><th>Asset</th><th className="text-right!">In</th><th className="text-right!">Out</th><th className="text-right!">Inside</th></tr>
                    </thead>
                    <tbody>
                      {numbers.flows.slice(0, 8).map((flow) => (
                        <tr key={flow.key}>
                          <td className="max-w-[64px] truncate font-mono-ui" title={flow.key}>{flow.symbol}</td>
                          <td className="text-right text-good">{flow.inflow > 0n ? formatUnits(flow.inflow, flow.decimals) : '—'}</td>
                          <td className="text-right text-critical">{flow.outflow > 0n ? formatUnits(flow.outflow, flow.decimals) : '—'}</td>
                          <td className="text-right text-text-secondary">{flow.internal > 0n ? formatUnits(flow.internal, flow.decimals) : '—'}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
                {numbers.flows.length === 0 && numbers.txs > 0 && <p className="m-0 text-text-muted">No value moved, only calls.</p>}
                <div className="flex flex-wrap gap-1">
                  <Button className="h-7 px-2 text-[11px]" disabled={group.nodes.length === 0 || group.hidden} onClick={() => onSelectMembers(group.nodes)}>
                    <IconTarget size={12} /> Select nodes
                  </Button>
                  <Button
                    variant="ghost"
                    className="h-7 px-2 text-[11px]"
                    disabled={selection.length === 0}
                    onClick={() => patch(group.id, { nodes: [...new Set([...group.nodes, ...selection])] })}
                  >
                    + selected
                  </Button>
                  <Button
                    variant="ghost"
                    className="h-7 px-2 text-[11px]"
                    disabled={selection.length === 0}
                    onClick={() => patch(group.id, { nodes: group.nodes.filter((id) => !selection.includes(id)) })}
                  >
                    − selected
                  </Button>
                  <Button variant="ghost" className="h-7 px-2 text-[11px]" onClick={() => setRenaming(group.id)}>Rename</Button>
                  <Button
                    variant="ghost"
                    className="ml-auto h-7 px-2 text-[11px] text-critical"
                    onClick={() => onGroupsChange(groups.filter((candidate) => candidate.id !== group.id))}
                  >
                    Ungroup
                  </Button>
                </div>
              </div>
            )}
          </PanelSection>
        );
      })}

      <PanelSection>
        <PanelTitle>Hidden from canvas</PanelTitle>
        {hiddenNodes.size === 0 && hiddenTxs.size === 0 && <p className="m-0 text-xs text-text-muted">Nothing is hidden.</p>}
        {hiddenNodes.size > 0 && (
          <div className="grid gap-1">
            <div className="flex items-center justify-between text-xs">
              <span>{formatCount(hiddenNodes.size)} {hiddenNodes.size === 1 ? 'address' : 'addresses'}</span>
              <Button variant="ghost" className="h-7 px-2 text-[11px]" onClick={onShowAllNodes}>Show all</Button>
            </div>
            {(showAllHidden ? hiddenList : hiddenList.slice(0, HIDDEN_PREVIEW)).map((id) => (
              <div key={id} className="flex items-center justify-between gap-2 pl-2 text-[11px]">
                <span className="truncate font-mono-ui text-text-secondary" title={displayAddress(id)}>{shortAddress(displayAddress(id), 8, 6)}</span>
                <Button variant="ghost" className="h-6 px-1.5 text-[11px]" onClick={() => onShowNode(id)}>Show</Button>
              </div>
            ))}
            {hiddenList.length > HIDDEN_PREVIEW && (
              <Button variant="ghost" className="h-6 justify-self-start px-1.5 text-[11px]" onClick={() => setShowAllHidden(!showAllHidden)}>
                {showAllHidden ? 'Show fewer' : `${hiddenList.length - HIDDEN_PREVIEW} more`}
              </Button>
            )}
          </div>
        )}
        {hiddenTxs.size > 0 && (
          <div className="mt-1 flex items-center justify-between text-xs">
            <span>{formatCount(hiddenTxs.size)} {hiddenTxs.size === 1 ? 'transaction' : 'transactions'}</span>
            <Button variant="ghost" className="h-7 px-2 text-[11px]" onClick={onShowAllTxs}>Show all</Button>
          </div>
        )}
        {groups.some((group) => group.hidden) && (
          <p className="mb-0 mt-2 text-[11px] text-text-muted">Hidden groups are listed above with a crossed-out eye.</p>
        )}
      </PanelSection>
    </>
  );
}
