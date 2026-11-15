import type { AddressLabel } from '../api/types';
import { shortAddress } from '../lib/format';

export interface LabelledAddress {
  id: string;
  systemLabels: readonly AddressLabel[];
}

/** The backend orders labels; the first non-empty value is the display identity. */
export function getPrimarySystemLabel(
  labels: readonly AddressLabel[] | null | undefined,
): AddressLabel | null {
  return labels?.find((label) => label.value.trim().length > 0) ?? null;
}

export function getNodeDisplayName(node: LabelledAddress): string {
  return getPrimarySystemLabel(node.systemLabels)?.value ?? shortAddress(node.id, 6, 4);
}

export function truncateCanvasLabel(value: string, maxLength = 24): string {
  if (value.length <= maxLength) return value;
  return `${value.slice(0, maxLength - 1).trimEnd()}…`;
}
