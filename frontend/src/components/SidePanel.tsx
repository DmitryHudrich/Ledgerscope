import type { ReactNode } from 'react';

import type { GraphRoot } from '../api/types';
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
  IconLegend,
  IconOverview,
  IconSearch,
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

export type WorkspacePanel = 'addresses' | 'filters' | 'legend' | 'stats';

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
}: Props) {
  const counts = { eoa: 0, contract: 0, focus: 0 };
  for (const node of model.nodes) counts[node.kind] += 1;

  const changePanel = (next: WorkspacePanel) =>
    onPanelChange(activePanel === next ? null : next);

  const title = activePanel
    ? ({ addresses: 'Addresses', filters: 'Filters', legend: 'Legend', stats: 'Graph stats' } as const)[activePanel]
    : '';

  return (
    <>
      <aside
        className="absolute bottom-[var(--sheet-h)] left-0 top-0 z-20 flex w-12 flex-col items-center border-r border-hairline bg-surface-1 transition-[bottom] duration-200 ease-[cubic-bezier(0.22,0.61,0.36,1)]"
        aria-label="Investigation rail"
      >
        <nav
          className="flex flex-col items-center gap-1 py-2"
          aria-label="Investigation sections"
        >
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
        </nav>
      </aside>

      {activePanel && (
        <Panel
          as="aside"
          className="absolute bottom-[calc(var(--sheet-h)+12px)] left-[60px] top-[var(--left-flyout-top)] z-15 flex w-[260px] animate-drawer-in flex-col overflow-hidden shadow-pop transition-[top] duration-150"
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
                search={search}
                onSearchChange={onSearchChange}
                matchCount={matchCount}
                onResetFilters={onResetFilters}
              />
            )}
            {activePanel === 'legend' && <LegendPanel counts={counts} />}
            {activePanel === 'stats' && <StatsPanel model={model} />}
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
                  title={node ? root.address : `${root.address} — no activity in the current graph`}
                  disabled={!node}
                  onClick={() => { onSelect(root.address); onCenter(root.address); }}
                >
                  <Dot tone={node?.kind ?? 'focus'} />
                  <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono-ui text-[11px]">
                    {shortAddress(root.address, 7, 4)}
                  </span>
                </button>
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

function FiltersPanel({ filters, onFiltersChange, search, onSearchChange, matchCount, onResetFilters }: Pick<Props, 'filters' | 'onFiltersChange' | 'search' | 'onSearchChange' | 'matchCount' | 'onResetFilters'>) {
  const patch = (next: Partial<GraphFilters>) => onFiltersChange({ ...filters, ...next });
  return (
    <PanelSection className="border-b-0 px-3 py-4">
      <PanelTitle>Graph filters</PanelTitle>
      <div className="mb-5 grid gap-[6px]">
        <label className="text-[10px] uppercase tracking-[0.07em] text-text-muted" htmlFor="f-search">Highlight address</label>
        <div className="relative">
          <IconSearch className="pointer-events-none absolute left-[9px] top-1/2 -translate-y-1/2 text-text-muted" size={14} />
          <Input id="f-search" className="w-full pl-[30px] font-mono-ui text-xs" value={search} spellCheck={false} autoComplete="off" placeholder="0x…" onChange={(event) => onSearchChange(event.target.value)} />
        </div>
        {search.trim().length > 0 && (
          <p className="m-0 text-[11px] text-text-muted">
            {matchCount > 0 ? `${formatCount(matchCount)} address${matchCount === 1 ? '' : 'es'} highlighted` : 'No match in the current graph'}
          </p>
        )}
      </div>
      <div className="mb-5 grid gap-2">
        <div className="flex items-baseline justify-between text-xs text-text-secondary">
          <label htmlFor="f-min">Minimum ETH flow</label>
          <span className="tabular-nums text-text-primary">{filters.minEth <= 0 ? 'Any' : `${formatCompact(filters.minEth)} ETH`}</span>
        </div>
        <RangeInput id="f-min" min={0} max={100} step={1} value={ethToSlider(filters.minEth)} onChange={(event) => patch({ minEth: sliderToEth(Number(event.target.value)) })} />
      </div>
      <div className="grid gap-1">
        <Checkbox checked={filters.showNative} onChange={(event) => patch({ showNative: event.target.checked })}>ETH transfers</Checkbox>
        <Checkbox checked={filters.showTokens} onChange={(event) => patch({ showTokens: event.target.checked })}>Token transfers</Checkbox>
        <Checkbox checked={filters.showCalls} onChange={(event) => patch({ showCalls: event.target.checked })}>Contract calls & deployments</Checkbox>
        <Checkbox checked={filters.hideIsolated} onChange={(event) => patch({ hideIsolated: event.target.checked })}>Hide unconnected addresses</Checkbox>
      </div>
      <div className="mt-4 flex items-center gap-1">
        <Button variant="ghost" onClick={() => patch({ showNative: true, showTokens: true, showCalls: false, minEth: 0 })}>Transfers only</Button>
        <Button variant="ghost" onClick={onResetFilters}>Reset filters</Button>
      </div>
    </PanelSection>
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
        <Stat label="ETH moved" value={formatEth(stats.volume)} unit="ETH" />
      </StatGrid>
      <MetaList className="mt-4">
        <div><dt>Token transfers</dt><dd>{formatCount(stats.tokenTransfers)}</dd></div>
        <div><dt>Contract calls</dt><dd>{formatCount(stats.calls)}</dd></div>
        <div><dt>Blocks</dt><dd>{stats.minBlock ? `${formatCount(stats.minBlock)} – ${formatCount(stats.maxBlock)}` : '—'}</dd></div>
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
              <div className="flex items-center gap-2 text-xs" key={flow.key} title={flow.native ? 'ETH' : flow.key}>
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

function LegendPanel({ counts }: { counts: Record<'eoa' | 'contract' | 'focus', number> }) {
  return (
    <PanelSection className="border-b-0 px-3 py-4">
      <PanelTitle>Graph legend</PanelTitle>
      <div className="grid gap-2.5">
        <LegendRow kind="focus" label="Focus wallet" count={counts.focus} />
        <LegendRow kind="eoa" label="Wallet (EOA)" count={counts.eoa} />
        <LegendRow kind="contract" label="Contract" count={counts.contract} square />
      </div>
      <div className="mt-4 grid gap-2.5 border-t border-hairline pt-4">
        <div className="flex items-center gap-[9px] text-xs text-text-secondary"><span className="h-[3px] w-[14px] flex-none rounded-[2px] bg-series-token" aria-hidden="true" />Token transfer edge</div>
        <div className="flex items-center gap-[9px] text-xs text-text-secondary"><span className="h-[3px] w-[14px] flex-none rounded-[2px] bg-[repeating-linear-gradient(90deg,var(--edge)_0_4px,transparent_4px_7px)]" aria-hidden="true" />Contract call edge</div>
      </div>
      <p className="mb-0 mt-4 text-[11px] leading-relaxed text-text-muted">Arrows point from sender to receiver. Node size represents turnover, counterparties and transaction count. Edge width represents flow and transaction count.</p>
    </PanelSection>
  );
}

function LegendRow({ kind, label, count, square }: { kind: 'eoa' | 'contract' | 'focus'; label: string; count: number; square?: boolean }) {
  return (
    <div className="flex items-center gap-[9px] text-xs text-text-secondary">
      <span className={cx('size-3 flex-none rounded-full border-[1.5px] border-plane shadow-[0_0_0_1px_var(--hairline)]', kind === 'eoa' && 'bg-series-eoa text-series-eoa', kind === 'contract' && 'bg-series-contract text-series-contract', kind === 'focus' && 'bg-series-focus text-series-focus shadow-[0_0_0_1px_currentColor]', square && 'rounded-[3px]')} aria-hidden="true" />
      {label}<span className="ml-auto tabular-nums text-text-muted">{formatCount(count)}</span>
    </div>
  );
}
