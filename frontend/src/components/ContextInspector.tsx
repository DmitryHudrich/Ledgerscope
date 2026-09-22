import { edgeLabel } from '../api/edges';
import { formatCount, formatTimestamp, formatUnits, shortAddress } from '../lib/format';
import type { GraphLink, GraphModel } from '../graph/model';
import { IconClose, IconTable } from './Icons';
import { Button, MetaList, Panel, PanelSection, PanelTitle, Tag } from './ui';

const shell = 'absolute bottom-[var(--sheet-h)] right-0 top-0 z-10 flex w-[var(--inspector-w)] animate-drawer-in flex-col overflow-hidden bg-surface-1 shadow-none backdrop-blur-none transition-[width] duration-200 ease-[cubic-bezier(0.22,0.61,0.36,1)]';

export function MultiSelectionInspector({
  ids,
  model,
  onRemove,
  onClear,
}: {
  ids: Set<string>;
  model: GraphModel;
  onRemove: () => void;
  onClear: () => void;
}) {
  const connections = model.links.filter((link) => ids.has(link.source.id) || ids.has(link.target.id)).length;
  return (
    <Panel as="aside" className={shell} style={{ borderRadius: 0, borderWidth: 0, borderLeftWidth: 1 }} aria-label="Selected addresses">
      <header className="flex items-start justify-between border-b border-hairline px-4 py-[14px]">
        <div><div className="text-[10px] uppercase tracking-[.07em] text-text-secondary">Selection</div><h2 className="m-0 mt-1 text-base font-semibold">{formatCount(ids.size)} addresses selected</h2></div>
        <Button variant="ghost" icon onClick={onClear} aria-label="Close selection"><IconClose /></Button>
      </header>
      <PanelSection>
        <MetaList className="mt-0"><div><dt>Visible connections</dt><dd>{formatCount(connections)}</dd></div></MetaList>
        <p className="mb-0 mt-3 text-xs leading-relaxed text-text-muted">Drag any selected node to move the group. Relative positions stay pinned.</p>
        <div className="mt-4 grid gap-2"><Button variant="primary" onClick={onRemove}>Remove from canvas</Button><Button onClick={onClear}>Clear selection</Button></div>
      </PanelSection>
    </Panel>
  );
}

export function EdgeInspector({ link, onClose, onShowTransactions }: { link: GraphLink; onClose: () => void; onShowTransactions: () => void }) {
  const first = link.txs[0];
  return (
    <Panel as="aside" className={shell} style={{ borderRadius: 0, borderWidth: 0, borderLeftWidth: 1 }} aria-label="Transfer details">
      <header className="flex items-start justify-between border-b border-hairline px-4 py-[14px]">
        <div><div className="text-[10px] uppercase tracking-[.07em] text-text-secondary">Transfer</div><h2 className="m-0 mt-1 text-sm font-semibold">{shortAddress(link.source.id, 7, 5)} → {shortAddress(link.target.id, 7, 5)}</h2></div>
        <Button variant="ghost" icon onClick={onClose} aria-label="Close transfer details"><IconClose /></Button>
      </header>
      <PanelSection>
        <PanelTitle>Interaction</PanelTitle>
        <MetaList>
          <div><dt>From</dt><dd className="font-mono-ui">{shortAddress(link.source.id, 9, 6)}</dd></div>
          <div><dt>To</dt><dd className="font-mono-ui">{shortAddress(link.target.id, 9, 6)}</dd></div>
          <div><dt>Transactions</dt><dd>{formatCount(link.count)}</dd></div>
          <div><dt>Type</dt><dd>{link.count === 1 && first ? edgeLabel(first) : 'Aggregated transfers'}</dd></div>
          {link.assets.length > 0 && <div><dt>Assets</dt><dd>{link.assets.map((asset) => <Tag key={asset.key}>{formatUnits(asset.amount, asset.decimals)} {asset.symbol}</Tag>)}</dd></div>}
          {link.count === 1 && first && <><div><dt>Block</dt><dd>{formatCount(first.block_number)}</dd></div><div><dt>Timestamp</dt><dd title={formatTimestamp(first.timestamp)}>{formatTimestamp(first.timestamp)}</dd></div></>}
        </MetaList>
        {link.count === 1 && first && <p className="mb-0 mt-3 break-all font-mono-ui text-[11px] text-text-muted">{first.tx_hash}</p>}
        <Button variant="primary" className="mt-4 w-full" onClick={onShowTransactions}><IconTable /> View transactions ({formatCount(link.count)})</Button>
      </PanelSection>
    </Panel>
  );
}
