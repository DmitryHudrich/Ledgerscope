import { useCallback, useSyncExternalStore } from 'react'

/**
 * How transfer edges are drawn on the canvas. Persisted globally (not per case)
 * because it's an analyst's viewing preference, not case data.
 */
export type EdgeDisplaySettings = {
  /** Render the amount/currency (or count) as text on each edge. */
  showLabels: boolean
  /** Tint each edge by its asset symbol so currencies are separable at a glance. */
  colorByCurrency: boolean
  /** Scale edge thickness by transfer amount. */
  widthByAmount: boolean
  /** Collapse parallel transfers of the same asset between a pair into one
   *  counted edge (`N× · total SYM`) instead of fanning them out. */
  aggregate: boolean
}

export const DEFAULT_EDGE_DISPLAY: EdgeDisplaySettings = {
  showLabels: true,
  colorByCurrency: true,
  widthByAmount: true,
  aggregate: false,
}

const STORAGE_KEY = 'ledgerscope.graph.edgeDisplay'

function read(): EdgeDisplaySettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) {
      return DEFAULT_EDGE_DISPLAY
    }
    return { ...DEFAULT_EDGE_DISPLAY, ...(JSON.parse(raw) as Partial<EdgeDisplaySettings>) }
  } catch {
    return DEFAULT_EDGE_DISPLAY
  }
}

let current = read()
const listeners = new Set<() => void>()

export function getEdgeDisplay(): EdgeDisplaySettings {
  return current
}

export function setEdgeDisplay(patch: Partial<EdgeDisplaySettings>): void {
  current = { ...current, ...patch }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(current))
  } catch {
    // ignore storage failures (private mode, quota)
  }
  listeners.forEach((listener) => listener())
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** Reactive accessor: `[settings, update]`. Shared across every mount. */
export function useEdgeDisplay(): [EdgeDisplaySettings, (patch: Partial<EdgeDisplaySettings>) => void] {
  const settings = useSyncExternalStore(subscribe, getEdgeDisplay, getEdgeDisplay)
  const update = useCallback((patch: Partial<EdgeDisplaySettings>) => setEdgeDisplay(patch), [])
  return [settings, update]
}

const CURRENCY_PALETTE = [
  '#7eb6ff',
  '#f5c26b',
  '#8fd694',
  '#c4b0f5',
  '#f09494',
  '#5fd0d6',
  '#f0a6d0',
  '#b6c96b',
] as const

/** Deterministic per-symbol colour so the same asset is always the same hue. */
export function colorForSymbol(symbol: string): string {
  let hash = 0
  const key = symbol.trim().toUpperCase() || '?'
  for (let index = 0; index < key.length; index += 1) {
    hash = (hash * 31 + key.charCodeAt(index)) >>> 0
  }
  return CURRENCY_PALETTE[hash % CURRENCY_PALETTE.length]
}
