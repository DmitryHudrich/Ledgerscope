import { formatCompact } from '../lib/format';
import { DEFAULT_FILTERS, type GraphFilters } from '../graph/model';

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
    labels.push(`Min flow ≥ ${formatCompact(filters.minEth)} ETH`);
  }
  if (filters.showNative !== DEFAULT_FILTERS.showNative) {
    labels.push(filters.showNative ? 'ETH transfers shown' : 'ETH transfers hidden');
  }
  if (filters.showTokens !== DEFAULT_FILTERS.showTokens) {
    labels.push(filters.showTokens ? 'Token transfers shown' : 'Token transfers hidden');
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
