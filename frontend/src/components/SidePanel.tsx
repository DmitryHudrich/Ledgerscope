import {
  formatCompact,
  formatCount,
  formatEth,
  formatTimestamp,
  formatUnits,
} from '../lib/format';
import { assetLabel, type GraphFilters, type GraphModel } from '../graph/model';
import { IconSearch } from './Icons';

const ASSET_PREVIEW = 6;

interface Props {
  model: GraphModel;
  filters: GraphFilters;
  onFiltersChange: (next: GraphFilters) => void;
  search: string;
  onSearchChange: (next: string) => void;
  matchCount: number;
}

export function sliderToEth(position: number): number {
  if (position <= 0) return 0;
  return 10 ** ((position / 100) * 6 - 4);
}

function ethToSlider(eth: number): number {
  if (eth <= 0) return 0;
  return Math.round(((Math.log10(eth) + 4) / 6) * 100);
}

export function SidePanel({
  model,
  filters,
  onFiltersChange,
  search,
  onSearchChange,
  matchCount,
}: Props) {
  const { stats } = model;
  const counts = { eoa: 0, contract: 0, focus: 0 };
  for (const node of model.nodes) counts[node.kind] += 1;

  const patch = (next: Partial<GraphFilters>) => onFiltersChange({ ...filters, ...next });

  return (
    <aside className="panel overlay sidepanel" aria-label="Graph summary and filters">
      <div className="sidepanel-scroll">
        <section className="panel-section">
          <h2 className="panel-title">Overview</h2>
          <div className="stat-grid">
            <Stat label="Addresses" value={formatCount(stats.nodes)} />
            <Stat label="Flows" value={formatCount(stats.links)} />
            <Stat label="Transfers" value={formatCount(stats.transfers)} />
            <Stat label="ETH moved" value={formatEth(stats.volume)} unit="ETH" />
          </div>
          <dl className="meta-list">
            <div>
              <dt>Token transfers</dt>
              <dd>{formatCount(stats.tokenTransfers)}</dd>
            </div>
            <div>
              <dt>Contract calls</dt>
              <dd>{formatCount(stats.calls)}</dd>
            </div>
            <div>
              <dt>Blocks</dt>
              <dd>
                {stats.minBlock ? `${formatCount(stats.minBlock)} – ${formatCount(stats.maxBlock)}` : '—'}
              </dd>
            </div>
            <div>
              <dt>First tx</dt>
              <dd>{formatTimestamp(stats.firstSeen)}</dd>
            </div>
            <div>
              <dt>Last tx</dt>
              <dd>{formatTimestamp(stats.lastSeen)}</dd>
            </div>
            {stats.failedTxs > 0 && (
              <div>
                <dt>Reverted</dt>
                <dd>{formatCount(stats.failedTxs)} tx</dd>
              </div>
            )}
            {stats.hiddenTxs > 0 && (
              <div>
                <dt>Filtered out</dt>
                <dd>{formatCount(stats.hiddenTxs)} tx</dd>
              </div>
            )}
          </dl>
        </section>

        <section className="panel-section">
          <h2 className="panel-title">Filters</h2>

          <div className="control">
            <label className="sr-only" htmlFor="f-search">
              Highlight addresses
            </label>
            <div className="search-wrap">
              <IconSearch className="search-icon" size={14} />
              <input
                id="f-search"
                className="input"
                value={search}
                spellCheck={false}
                autoComplete="off"
                placeholder="Highlight address…"
                onChange={(e) => onSearchChange(e.target.value)}
              />
            </div>
            {search.trim().length > 0 && (
              <p className="search-hits">
                {matchCount > 0
                  ? `${formatCount(matchCount)} address${matchCount === 1 ? '' : 'es'} highlighted`
                  : 'No match in the current graph'}
              </p>
            )}
          </div>

          <div className="control">
            <div className="control-head">
              <label htmlFor="f-min">Minimum ETH flow</label>
              <span className="control-value">
                {filters.minEth <= 0 ? 'any' : `${formatCompact(filters.minEth)} ETH`}
              </span>
            </div>
            <input
              id="f-min"
              type="range"
              min={0}
              max={100}
              step={1}
              value={ethToSlider(filters.minEth)}
              onChange={(e) => patch({ minEth: sliderToEth(Number(e.target.value)) })}
            />
          </div>

          <label className="check">
            <input
              type="checkbox"
              checked={filters.showNative}
              onChange={(e) => patch({ showNative: e.target.checked })}
            />
            ETH transfers
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={filters.showTokens}
              onChange={(e) => patch({ showTokens: e.target.checked })}
            />
            Token transfers
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={filters.showCalls}
              onChange={(e) => patch({ showCalls: e.target.checked })}
            />
            Contract calls & deployments
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={filters.hideIsolated}
              onChange={(e) => patch({ hideIsolated: e.target.checked })}
            />
            Hide unconnected addresses
          </label>

          <button
            type="button"
            className="btn btn-ghost"
            style={{ marginTop: 8 }}
            onClick={() =>
              patch({ showNative: true, showTokens: true, showCalls: false, minEth: 0 })
            }
          >
            Transfers only
          </button>
        </section>

        {stats.assets.length > 0 && (
          <section className="panel-section">
            <h2 className="panel-title">Assets moved · {formatCount(stats.assets.length)}</h2>
            <div className="asset-list">
              {stats.assets.slice(0, ASSET_PREVIEW).map((flow) => (
                <div className="asset-row" key={flow.key} title={flow.native ? 'ETH' : flow.key}>
                  <span
                    className={`dot dot-${flow.native ? 'focus' : 'token'}`}
                    aria-hidden="true"
                  />
                  <span className="asset-symbol">{assetLabel(flow)}</span>
                  <span className="asset-amount">{formatUnits(flow.amount, flow.decimals)}</span>
                  <span className="asset-count">{formatCount(flow.count)} tx</span>
                </div>
              ))}
            </div>
            {stats.assets.length > ASSET_PREVIEW && (
              <p className="legend-note">
                +{formatCount(stats.assets.length - ASSET_PREVIEW)} more assets in this range.
              </p>
            )}
          </section>
        )}

        <section className="panel-section">
          <h2 className="panel-title">Legend</h2>
          <div className="legend">
            <LegendRow kind="focus" label="Focus wallet" count={counts.focus} />
            <LegendRow kind="eoa" label="Wallet (EOA)" count={counts.eoa} />
            <LegendRow kind="contract" label="Contract" count={counts.contract} square />
          </div>
          <div className="legend">
            <div className="legend-row">
              <span className="swatch swatch-line swatch-token" aria-hidden="true" />
              Token transfer edge
            </div>
            <div className="legend-row">
              <span className="swatch swatch-line swatch-dashed" aria-hidden="true" />
              Contract call edge
            </div>
          </div>
          <p className="legend-note">
            Edges connect sender to receiver: for an ERC-20 transfer that is the token's{' '}
            <code className="mono">from</code>/<code className="mono">to</code>, not the contract
            you called. Node size = ETH turnover, counterparties and tx count · edge width = ETH
            moved and tx count · the minimum-flow slider only filters ETH-only edges.
          </p>
        </section>
      </div>
    </aside>
  );
}

function Stat({ label, value, unit }: { label: string; value: string; unit?: string }) {
  return (
    <div className="stat">
      <div className="stat-label">{label}</div>
      <div className="stat-value">
        {value}
        {unit && <span className="stat-unit">{unit}</span>}
      </div>
    </div>
  );
}

function LegendRow({
  kind,
  label,
  count,
  square,
}: {
  kind: 'eoa' | 'contract' | 'focus';
  label: string;
  count: number;
  square?: boolean;
}) {
  return (
    <div className="legend-row">
      <span
        className={`swatch swatch-${kind}${square ? ' swatch-square' : ''}${
          kind === 'focus' ? ' swatch-ring' : ''
        }`}
        aria-hidden="true"
      />
      {label}
      <span className="legend-count">{formatCount(count)}</span>
    </div>
  );
}
