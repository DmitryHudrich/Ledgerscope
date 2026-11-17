import { formatCompact, formatCount } from '../lib/format';
import { DEFAULT_FILTERS, type GraphFilters } from '../graph/model';
import { nativeSymbol } from '../lib/chain';

export interface FilterActivity {
  activeFilterCount: number;
  hasActiveFilters: boolean;
  summary: string;
}

export function getFilterActivity(
  filters: GraphFilters,
  highlightAddress: string,
): FilterActivity {
  const labels: string[] = [];

  if (highlightAddress.trim() !== '') labels.push('Address highlight');
  if (filters.minEth !== DEFAULT_FILTERS.minEth) {
    labels.push(`Min flow ≥ ${formatCompact(filters.minEth)} ${nativeSymbol()}`);
  }
  if (filters.showNative !== DEFAULT_FILTERS.showNative) {
    labels.push(`${nativeSymbol()} transfers ${filters.showNative ? 'shown' : 'hidden'}`);
  }
  if (filters.showTokens !== DEFAULT_FILTERS.showTokens) {
    labels.push(filters.showTokens ? 'Token transfers shown' : 'Token transfers hidden');
  }
  if (filters.showSwaps !== DEFAULT_FILTERS.showSwaps) {
    labels.push(filters.showSwaps ? 'Swaps shown' : 'Swaps hidden');
  }
  if (filters.showCalls !== DEFAULT_FILTERS.showCalls) {
    labels.push(
      !filters.showCalls && filters.showNative && filters.showTokens
        ? 'Transfers only'
        : filters.showCalls
          ? 'Contract calls shown'
          : 'Contract calls hidden',
    );
  }
  if (!filters.showDeployments) labels.push('Deployments hidden');
  if (!filters.showFailed) labels.push('Reverted hidden');
  if (filters.hideSelfLoops) labels.push('Self transfers hidden');
  if (!filters.showEoas) labels.push('Wallets hidden');
  if (!filters.showContracts) labels.push('Contracts hidden');
  if (filters.asset.trim() !== '') labels.push(`Asset ${filters.asset.trim()}`);
  if (filters.labels !== 'any') labels.push(filters.labels === 'labeled' ? 'Labeled only' : 'Unlabeled only');
  if (filters.minDegree > 1) labels.push(`≥ ${filters.minDegree} counterparties`);
  if (filters.minLinkTxs > 1) labels.push(`≥ ${filters.minLinkTxs} txs per flow`);
  if (filters.maxHops !== null) labels.push(`≤ ${filters.maxHops} hops`);
  if (filters.tag.trim() !== '') labels.push(`Tag ${filters.tag.trim()}`);
  if (filters.window) labels.push(`Blocks ${formatCount(filters.window.from)}–${formatCount(filters.window.to)}`);
  if (filters.hideIsolated !== DEFAULT_FILTERS.hideIsolated) {
    labels.push(filters.hideIsolated ? 'Unconnected hidden' : 'Unconnected shown');
  }

  const activeFilterCount = labels.length;
  return {
    activeFilterCount,
    hasActiveFilters: activeFilterCount > 0,
    summary:
      activeFilterCount <= 2
        ? labels.join(' · ')
        : `${activeFilterCount} filters active`,
  };
}
