import { useState, type ReactNode } from 'react';

import type { GraphRoot } from '../api/types';
import type { IpWalletLink } from '../api/tls';
import type { LabelMode } from '../graph/draw';
import { assetLabel, type GraphFilters, type GraphModel } from '../graph/model';
import {
  formatCompact,
  formatCount,
  formatEth,
  formatTimestamp,
  formatUnits,
  shortAddress,
} from '../lib/format';
import {
  IconClose,
  IconFilter,
  IconFit,
  IconGroups,
  IconLegend,
  IconOverview,
  IconSearch,
  IconTarget,
} from './Icons';
import {
  Badge,
  Button,
  Checkbox,
  Dot,
  Input,
  MetaList,
  Panel,
  PanelSection,
  PanelTitle,
  RangeInput,
  Stat,
  StatGrid,
  Tooltip,
  cx,
} from './ui';
import { displayAddress, nativeSymbol } from '../lib/chain';
import { BlockSpanText } from './BlockSpanText';
import { CopyAddress } from './CopyAddress';

export type WorkspacePanel = 'advanced' | 'addresses' | 'filters' | 'legend' | 'stats' | 'groups' | 'tls';

interface Props {
  model: GraphModel;
  roots: GraphRoot[];
  maxDepth: number;
  filters: GraphFilters;
  onFiltersChange: (next: GraphFilters) => void;
  search: string;
  onSearchChange: (next: string) => void;
  matchCount: number;
  activePanel: WorkspacePanel | null;
  onPanelChange: (next: WorkspacePanel | null) => void;
  activeFilterCount: number;
  onResetFilters: () => void;
  onRootRemove: (address: string) => void;
  onRootDepthChange: (address: string, depth: number) => void;
  onSelect: (id: string) => void;
  onCenter: (id: string) => void;
  labelMode: LabelMode;
  onLabelModeChange: (next: LabelMode) => void;
  groupsPanel?: ReactNode;
  groupsBadge?: number;
  advancedPanel?: ReactNode;
  ipLinks?: Map<string, IpWalletLink[]>;
}

export function sliderToEth(position: number): number {
  if (position <= 0) return 0;
  return 10 ** ((position / 100) * 6 - 4);
}

function ethToSlider(eth: number): number {
  if (eth <= 0) return 0;
  return Math.round(((Math.log10(eth) + 4) / 6) * 100);
}

export function InvestigationPanel({
  model,
  roots,
  maxDepth,
  filters,
  onFiltersChange,
  search,
  onSearchChange,
  matchCount,
  activePanel,
  onPanelChange,
  activeFilterCount,
  onResetFilters,
  onRootRemove,
  onRootDepthChange,
  onSelect,
  onCenter,
  labelMode,
  onLabelModeChange,
  groupsPanel,
  groupsBadge = 0,
  advancedPanel,
  ipLinks,
}: Props) {
  const counts = { eoa: 0, contract: 0, root: 0 };
  for (const node of model.nodes) {
    counts[node.kind] += 1;
    if (node.root) counts.root += 1;
  }

  const changePanel = (next: WorkspacePanel) =>
    onPanelChange(activePanel === next ? null : next);

  const title = activePanel
    ? ({ advanced: 'Block range', addresses: 'Addresses', filters: 'Filters', legend: 'Legend', stats: 'Graph stats', groups: 'Groups & hidden', tls: 'IP links' } as const)[activePanel]
    : '';

  return (
    <>
      <aside
        className="absolute bottom-[var(--sheet-h)] left-0 top-0 z-20 flex w-12 flex-col items-center border-r border-hairline bg-surface-1"
        aria-label="Investigation rail"
      >
        <nav
          className="flex flex-col items-center gap-1 py-2"
          aria-label="Investigation sections"
        >
          {advancedPanel && (
            <TabButton label="Block range" active={activePanel === 'advanced'} onClick={() => changePanel('advanced')}>
              <IconFit />
            </TabButton>
          )}
          <TabButton label="Addresses" active={activePanel === 'addresses'} onClick={() => changePanel('addresses')}>
            <IconSearch />
          </TabButton>
          <TabButton label="Filters" active={activePanel === 'filters'} badge={activeFilterCount} onClick={() => changePanel('filters')}>
            <IconFilter />
          </TabButton>
          <TabButton label="Legend" active={activePanel === 'legend'} onClick={() => changePanel('legend')}>
            <IconLegend />
          </TabButton>
          <TabButton label="Stats" active={activePanel === 'stats'} onClick={() => changePanel('stats')}>
            <IconOverview />
          </TabButton>
          <TabButton label="IP links" active={activePanel === 'tls'} onClick={() => changePanel('tls')}>
            <IconTarget />
          </TabButton>
          {groupsPanel && (
            <TabButton label="Groups & hidden" active={activePanel === 'groups'} badge={groupsBadge} onClick={() => changePanel('groups')}>
              <IconGroups />
            </TabButton>
          )}
        </nav>
      </aside>

      {activePanel && (
        <Panel
          as="aside"
          className={cx("absolute bottom-[calc(var(--sheet-h)+12px)] left-[60px] top-[var(--left-flyout-top)] z-15 flex animate-drawer-in flex-col overflow-hidden shadow-pop transition-[top] duration-150", activePanel === "groups" || activePanel === "advanced" ? "w-[300px]" : "w-[260px]")}
          aria-label={`${title} flyout`}
          data-left-flyout
        >
          <header className="flex h-11 flex-none items-center justify-between border-b border-hairline px-[14px]">
            <h2 className="m-0 text-[11px] font-semibold uppercase tracking-[0.08em] text-text-secondary">
              {title}
            </h2>
            <Button variant="ghost" icon className="size-7" onClick={() => onPanelChange(null)} aria-label={`Close ${title}`}>
              <IconClose size={14} />
            </Button>
          </header>
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
            {activePanel === 'advanced' && advancedPanel}
            {activePanel === 'addresses' && (
              <AddressesPanel
                model={model}
                roots={roots}
                maxDepth={maxDepth}
                onRemove={onRootRemove}
                onDepthChange={onRootDepthChange}
                onSelect={(id) => {
                  onSelect(id);
                  onPanelChange(null);
                }}
                onCenter={onCenter}
              />
            )}
            {activePanel === 'filters' && (
              <FiltersPanel
                filters={filters}
                onFiltersChange={onFiltersChange}
                model={model}
                search={search}
                onSearchChange={onSearchChange}
                matchCount={matchCount}
                onResetFilters={onResetFilters}
              />
            )}
            {activePanel === 'legend' && (
              <LegendPanel
                counts={counts}
                showLabels={labelMode !== 'none'}
                onShowLabelsChange={(show) => onLabelModeChange(show ? 'auto' : 'none')}
              />
            )}
            {activePanel === 'stats' && <StatsPanel model={model} />}
            {activePanel === 'tls' && (
              <TlsPanel
                model={model}
                ipLinks={ipLinks}
                onSelect={(id) => {
                  onSelect(id);
                  onPanelChange(null);
                }}
                onCenter={onCenter}
              />
            )}
            {activePanel === 'groups' && groupsPanel}
          </div>
        </Panel>
      )}
    </>
  );
}

function TabButton({ label, active, badge = 0, onClick, children }: {
  label: string;
  active: boolean;
  badge?: number;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Tooltip label={label} disabled={active}>
      <Button
        variant="ghost"
        icon
        className={cx(
          'relative size-9',
          active && 'bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] text-accent',
        )}
        aria-label={badge > 0 ? `${label}, ${badge} active` : label}
        aria-current={active ? 'page' : undefined}
        onClick={onClick}
      >
        {children}
        {badge > 0 && (
          <span
            className="absolute -right-0.5 -top-0.5 grid min-w-[15px] place-content-center rounded-full bg-accent px-1 text-[9px] font-semibold leading-[15px] text-accent-ink shadow-[0_0_0_2px_var(--surface-1)]"
            aria-hidden="true"
          >
            {badge}
          </span>
        )}
      </Button>
    </Tooltip>
  );
}

function AddressesPanel({ model, roots, maxDepth, onSelect, onCenter, onRemove, onDepthChange }: {
  model: GraphModel;
  roots: GraphRoot[];
  maxDepth: number;
  onSelect: (id: string) => void;
  onCenter: (id: string) => void;
  onRemove: (address: string) => void;
  onDepthChange: (address: string, depth: number) => void;
}) {
  return (
    <PanelSection className="border-b-0 px-3 py-4">
      <div className="mb-3 flex items-center justify-between">
        <PanelTitle className="mb-0">Addresses</PanelTitle>
        <Badge>{formatCount(roots.length)}</Badge>
      </div>
      {roots.length === 0 ? (
        <p className="m-0 text-xs leading-relaxed text-text-muted">Add an address above to start an investigation.</p>
      ) : (
        <div className="grid gap-1">
          {roots.map((root) => {
            const node = model.byId.get(root.address);
            return (
              <div key={root.address} className="flex items-center gap-1 rounded-ui-sm px-1 py-1 hover:bg-[color-mix(in_srgb,var(--text-primary)_5%,transparent)]">
                <button
                  type="button"
                  className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-ui-sm border-0 bg-transparent px-1 py-1.5 text-left focus-visible:outline-2 focus-visible:outline-accent disabled:cursor-default disabled:opacity-55"
                  title={node ? displayAddress(root.address) : `${displayAddress(root.address)} — no activity in the current graph`}
                  disabled={!node}
                  onClick={() => { onSelect(root.address); onCenter(root.address); }}
                >
                  <Dot tone={node?.kind ?? 'eoa'} />
                  <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono-ui text-[11px]">
                    {shortAddress(root.address, 7, 4)}
                  </span>
                </button>
                <CopyAddress address={root.address} />
                <div className="flex items-center text-[10px] text-text-muted" title="Investigation depth">
                  <Button variant="ghost" icon className="size-6" disabled={root.depth <= 0} aria-label={`Less depth for ${root.address}`} onClick={() => onDepthChange(root.address, root.depth - 1)}>−</Button>
                  <span className="w-4 text-center tabular-nums">{root.depth}</span>
                  <Button variant="ghost" icon className="size-6" disabled={root.depth >= maxDepth} aria-label={`More depth for ${root.address}`} onClick={() => onDepthChange(root.address, root.depth + 1)}>+</Button>
                </div>
                <Button variant="ghost" icon className="size-6" aria-label={`Remove ${root.address}`} onClick={() => onRemove(root.address)}>
                  <IconClose size={12} />
                </Button>
              </div>
            );
          })}
        </div>
      )}
    </PanelSection>
  );
}

function TlsPanel({ model, ipLinks, onSelect, onCenter }: {
  model: GraphModel;
  ipLinks?: Map<string, IpWalletLink[]>;
  onSelect: (id: string) => void;
  onCenter: (id: string) => void;
}) {
  const [query, setQuery] = useState('');
  const needle = query.trim().toLowerCase();
  const rows = [...(ipLinks?.entries() ?? [])]
    .filter(([address]) => model.byId.has(address))
    .filter(
      ([address, links]) =>
        !needle || address.includes(needle) || links.some((link) => link.src_ip.toLowerCase().includes(needle)),
    )
    .map(([address, links]) => ({ address, links, top: Math.max(...links.map((link) => link.score)) }))
    .sort((a, b) => b.top - a.top);

  return (
    <PanelSection className="border-b-0 px-3 py-4">
      <div className="mb-3 flex items-center justify-between">
        <PanelTitle className="mb-0">IP ↔ wallet</PanelTitle>
        <Badge>{formatCount(rows.length)}</Badge>
      </div>
      <Input
        className="mb-3 w-full font-mono-ui text-xs"
        placeholder="filter ip or address"
        value={query}
        spellCheck={false}
        autoComplete="off"
        onChange={(event) => setQuery(event.target.value)}
      />
      {rows.length === 0 ? (
        <p className="m-0 text-xs leading-relaxed text-text-muted">
          No ip links among the nodes on this canvas yet. Run correlation from the account menu → TLS deanon.
        </p>
      ) : (
        <div className="grid gap-1.5">
          {rows.map(({ address, links }) => (
            <div key={address} className="relative">
            <CopyAddress address={address} className="absolute right-1.5 top-1.5" />
            <button
              type="button"
              className="grid w-full gap-1 rounded-ui-sm px-2 py-2 pr-8 text-left hover:bg-[color-mix(in_srgb,var(--text-primary)_5%,transparent)] focus-visible:outline-2 focus-visible:outline-accent"
              onClick={() => { onSelect(address); onCenter(address); }}
            >
              <span className="overflow-hidden text-ellipsis whitespace-nowrap font-mono-ui text-[11px] font-medium">
                {shortAddress(address, 8, 6)}
              </span>
              {links.slice(0, 4).map((link) => (
                <span key={link.src_ip} className="flex items-center justify-between gap-2 pl-2 font-mono-ui text-[11px] text-text-secondary">
                  <span className="truncate">{link.src_ip}</span>
                  <span className="tabular-nums text-text-muted" title={`${link.hits}/${link.address_txs} tx in window`}>{Math.round(link.score * 100)}%</span>
                </span>
              ))}
            </button>
            </div>
          ))}
        </div>
      )}
    </PanelSection>
  );
}

function FiltersPanel({ filters, onFiltersChange, model, search, onSearchChange, matchCount, onResetFilters }: Pick<Props, 'filters' | 'onFiltersChange' | 'model' | 'search' | 'onSearchChange' | 'matchCount' | 'onResetFilters'>) {
  const patch = (next: Partial<GraphFilters>) => onFiltersChange({ ...filters, ...next });
  const symbols = [...new Set(model.stats.assets.filter((flow) => !flow.native).map((flow) => flow.symbol))].slice(0, 40);
  return (
    <PanelSection className="border-b-0 px-3 py-4">
      <PanelTitle>Graph filters</PanelTitle>
      <div className="mb-5 grid gap-[6px]">
        <label className="text-[10px] uppercase tracking-[0.07em] text-text-muted" htmlFor="f-search">Highlight address or label</label>
        <div className="relative">
          <IconSearch className="pointer-events-none absolute left-[9px] top-1/2 -translate-y-1/2 text-text-muted" size={14} />
          <Input id="f-search" className="w-full pl-[30px] font-mono-ui text-xs" value={search} spellCheck={false} autoComplete="off" placeholder="0x…" onChange={(event) => onSearchChange(event.target.value)} />
        </div>
        {search.trim().length > 0 && (
          <p className="m-0 text-[11px] text-text-muted">
            {matchCount > 0 ? `${formatCount(matchCount)} node${matchCount === 1 ? '' : 's'} highlighted` : 'No match in the current graph'}
          </p>
        )}
      </div>

      <FilterGroup title="Transactions">
        <Checkbox checked={filters.showNative} onChange={(event) => patch({ showNative: event.target.checked })}>{nativeSymbol()} transfers</Checkbox>
        <Checkbox checked={filters.showTokens} onChange={(event) => patch({ showTokens: event.target.checked })}>Token transfers</Checkbox>
        <Checkbox checked={filters.showSwaps} onChange={(event) => patch({ showSwaps: event.target.checked })}>Swaps</Checkbox>
        <Checkbox checked={filters.showCalls} onChange={(event) => patch({ showCalls: event.target.checked })}>Contract calls</Checkbox>
        <Checkbox checked={filters.showDeployments} onChange={(event) => patch({ showDeployments: event.target.checked })}>Deployments</Checkbox>
        <Checkbox checked={filters.showFailed} onChange={(event) => patch({ showFailed: event.target.checked })}>Reverted transactions</Checkbox>
        <Checkbox checked={!filters.hideSelfLoops} onChange={(event) => patch({ hideSelfLoops: !event.target.checked })}>Self transfers</Checkbox>
      </FilterGroup>

      <FilterGroup title="Value">
        <div className="grid gap-2">
          <div className="flex items-baseline justify-between text-xs text-text-secondary">
            <label htmlFor="f-min">Minimum {nativeSymbol()} flow</label>
            <span className="tabular-nums text-text-primary">{filters.minEth <= 0 ? 'Any' : `${formatCompact(filters.minEth)} ${nativeSymbol()}`}</span>
          </div>
          <RangeInput id="f-min" min={0} max={100} step={1} value={ethToSlider(filters.minEth)} onChange={(event) => patch({ minEth: sliderToEth(Number(event.target.value)) })} />
        </div>
        <div className="mt-2 grid gap-[6px]">
          <label className="text-xs text-text-secondary" htmlFor="f-asset">Only this asset</label>
          <Input id="f-asset" list="f-asset-list" className="w-full font-mono-ui text-xs" value={filters.asset} spellCheck={false} autoComplete="off" placeholder="USDT, 0x… or empty for all" onChange={(event) => patch({ asset: event.target.value })} />
          <datalist id="f-asset-list">
            <option value={nativeSymbol()} />
            {symbols.map((symbol) => <option key={symbol} value={symbol} />)}
          </datalist>
        </div>
      </FilterGroup>

      <FilterGroup title="Addresses">
        <Checkbox checked={filters.showEoas} onChange={(event) => patch({ showEoas: event.target.checked })}>Wallets (EOA)</Checkbox>
        <Checkbox checked={filters.showContracts} onChange={(event) => patch({ showContracts: event.target.checked })}>Contracts</Checkbox>
        <Checkbox checked={filters.hideIsolated} onChange={(event) => patch({ hideIsolated: event.target.checked })}>Hide unconnected addresses</Checkbox>
        <div className="mt-2 grid grid-cols-3 gap-1" role="radiogroup" aria-label="Labels">
          {(['any', 'labeled', 'unlabeled'] as const).map((value) => (
            <Button key={value} variant="ghost" className={cx('h-7 px-1 text-[11px]', filters.labels === value && 'bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] text-accent')} role="radio" aria-checked={filters.labels === value} onClick={() => patch({ labels: value })}>
              {value === 'any' ? 'Any label' : value === 'labeled' ? 'Labeled' : 'Unlabeled'}
            </Button>
          ))}
        </div>
      </FilterGroup>

      {model.stats.tagKeys.length > 0 || filters.tag !== '' ? (
        <FilterGroup title="Tags">
          <label className="text-xs text-text-secondary" htmlFor="f-tag">Only flows tagged</label>
          <select
            id="f-tag"
            className="h-8 w-full rounded-ui-sm border border-hairline bg-surface-2 px-2 text-xs text-text-primary"
            value={filters.tag}
            onChange={(event) => patch({ tag: event.target.value })}
          >
            <option value="">Any tag or none</option>
            {[...new Set(model.stats.tagKeys.map((key) => key.split('.')[0]))].map((category) => (
              <optgroup key={category} label={category}>
                <option value={category}>All {category}</option>
                {model.stats.tagKeys.filter((key) => key.startsWith(`${category}.`)).map((key) => (
                  <option key={key} value={key}>{key}</option>
                ))}
              </optgroup>
            ))}
          </select>
        </FilterGroup>
      ) : null}

      <FilterGroup title="Structure">
        <Stepper id="f-degree" label="Min counterparties" value={filters.minDegree} min={0} max={50} format={(value) => (value <= 1 ? 'Any' : String(value))} onChange={(minDegree) => patch({ minDegree })} />
        <Stepper id="f-link" label="Min txs per flow" value={filters.minLinkTxs} min={0} max={100} format={(value) => (value <= 1 ? 'Any' : String(value))} onChange={(minLinkTxs) => patch({ minLinkTxs })} />
        <Stepper id="f-hops" label="Max hops from roots" value={filters.maxHops ?? -1} min={-1} max={10} format={(value) => (value < 0 ? 'Any' : String(value))} onChange={(value) => patch({ maxHops: value < 0 ? null : value })} />
      </FilterGroup>

      <div className="mt-4 flex flex-wrap items-center gap-1">
        <Button variant="ghost" onClick={() => patch({ showNative: true, showTokens: true, showSwaps: false, showCalls: false, showDeployments: false, minEth: 0 })}>Transfers only</Button>
        <Button variant="ghost" onClick={onResetFilters}>Reset filters</Button>
      </div>
    </PanelSection>
  );
}

function FilterGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="mb-4 grid gap-1 border-t border-hairline pt-3">
      <h3 className="m-0 mb-1 text-[10px] font-semibold uppercase tracking-[0.07em] text-text-muted">{title}</h3>
      {children}
    </div>
  );
}

function Stepper({ id, label, value, min, max, format, onChange }: {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  format: (value: number) => string;
  onChange: (value: number) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-2 py-[2px] text-xs text-text-secondary">
      <label htmlFor={id}>{label}</label>
      <div className="flex items-center">
        <Button variant="ghost" icon className="size-6" disabled={value <= min} aria-label={`Less ${label}`} onClick={() => onChange(value - 1)}>−</Button>
        <output id={id} className="w-9 text-center tabular-nums text-text-primary">{format(value)}</output>
        <Button variant="ghost" icon className="size-6" disabled={value >= max} aria-label={`More ${label}`} onClick={() => onChange(value + 1)}>+</Button>
      </div>
    </div>
  );
}

function StatsPanel({ model }: { model: GraphModel }) {
  const { stats } = model;
  return (
    <PanelSection className="border-b-0 p-4">
      <StatGrid>
        <Stat label="Addresses" value={formatCount(stats.nodes)} />
        <Stat label="Flows" value={formatCount(stats.links)} />
        <Stat label="Transfers" value={formatCount(stats.transfers)} />
        <Stat label={`${nativeSymbol()} moved`} value={formatEth(stats.volume)} unit={nativeSymbol()} />
      </StatGrid>
      <MetaList className="mt-4">
        <div><dt>Token transfers</dt><dd>{formatCount(stats.tokenTransfers)}</dd></div>
        <div><dt>Swaps</dt><dd>{formatCount(stats.swaps)}</dd></div>
        <div><dt>Contract calls</dt><dd>{formatCount(stats.calls)}</dd></div>
        <div><dt>Blocks</dt><dd>{stats.minBlock ? <BlockSpanText from={stats.minBlock} to={stats.maxBlock} /> : '—'}</dd></div>
        <div><dt>First tx</dt><dd>{formatTimestamp(stats.firstSeen)}</dd></div>
        <div><dt>Last tx</dt><dd>{formatTimestamp(stats.lastSeen)}</dd></div>
        {stats.failedTxs > 0 && <div><dt>Reverted</dt><dd>{formatCount(stats.failedTxs)} tx</dd></div>}
        {stats.hiddenTxs > 0 && <div><dt>Filtered out</dt><dd>{formatCount(stats.hiddenTxs)} tx</dd></div>}
      </MetaList>
      <div className="mt-4 border-t border-hairline pt-4">
        <PanelTitle>Assets moved · {formatCount(stats.assets.length)}</PanelTitle>
        {stats.assets.length === 0 ? (
          <p className="m-0 text-xs text-text-muted">No assets match the current filters.</p>
        ) : (
          <div className="grid gap-1.5">
            {stats.assets.map((flow) => (
              <div className="flex items-center gap-2 text-xs" key={flow.key} title={flow.native ? nativeSymbol() : flow.key}>
                <Dot tone={flow.native ? 'focus' : 'token'} />
                <span className="max-w-[92px] overflow-hidden text-ellipsis whitespace-nowrap font-mono-ui">{assetLabel(flow)}</span>
                <span className="ml-auto tabular-nums">{formatUnits(flow.amount, flow.decimals)}</span>
                <span className="min-w-14 text-right tabular-nums text-text-muted">{formatCount(flow.count)} tx</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </PanelSection>
  );
}

function LegendPanel({ counts, showLabels, onShowLabelsChange }: {
  counts: Record<'eoa' | 'contract' | 'root', number>;
  showLabels: boolean;
  onShowLabelsChange: (show: boolean) => void;
}) {
  return (
    <PanelSection className="border-b-0 px-3 py-4">
      <PanelTitle>Graph legend</PanelTitle>
      <div className="grid gap-2.5">
        <LegendRow kind="eoa" label="Wallet (EOA)" count={counts.eoa} />
        <LegendRow kind="contract" label="Contract" count={counts.contract} square />
        <div className="flex items-center gap-[9px] text-xs text-text-secondary">
          <span className="size-3 flex-none rounded-full border-[1.5px] border-text-muted" aria-hidden="true" />
          Investigation root<span className="ml-auto tabular-nums text-text-muted">{formatCount(counts.root)}</span>
        </div>
      </div>
      <div className="mt-4 grid gap-2.5 border-t border-hairline pt-4">
        <div className="flex items-center gap-[9px] text-xs text-text-secondary"><span className="h-[3px] w-[14px] flex-none rounded-[2px] bg-series-token" aria-hidden="true" />Token transfer edge</div>
        <div className="flex items-center gap-[9px] text-xs text-text-secondary"><span className="h-[3px] w-[14px] flex-none rounded-[2px] bg-[repeating-linear-gradient(90deg,var(--edge)_0_4px,transparent_4px_7px)]" aria-hidden="true" />Contract call edge</div>
      </div>
      <div className="mt-4 border-t border-hairline pt-3">
        <Checkbox checked={showLabels} onChange={(event) => onShowLabelsChange(event.target.checked)}>
          Show labels
        </Checkbox>
      </div>
      <p className="mb-0 mt-4 text-[11px] leading-relaxed text-text-muted">Arrows point from sender to receiver. Node size represents turnover, counterparties and transaction count. Edge width represents flow and transaction count.</p>
    </PanelSection>
  );
}

function LegendRow({ kind, label, count, square }: { kind: 'eoa' | 'contract'; label: string; count: number; square?: boolean }) {
  return (
    <div className="flex items-center gap-[9px] text-xs text-text-secondary">
      <span className={cx('size-3 flex-none rounded-full border-[1.5px] border-plane shadow-[0_0_0_1px_var(--hairline)]', kind === 'eoa' && 'bg-series-eoa text-series-eoa', kind === 'contract' && 'bg-series-contract text-series-contract', square && 'rounded-[3px]')} aria-hidden="true" />
      {label}<span className="ml-auto tabular-nums text-text-muted">{formatCount(count)}</span>
    </div>
  );
}
