import type { GraphResponse, GraphRoot, GraphScope } from '../api/types';
import type { Annotation } from '../graph/annotations';
import type { LabelMode } from '../graph/draw';
import { DEFAULT_FILTERS, type GraphFilters, type NodePosition } from '../graph/model';
import type { BlockSpan } from './blockRange';
import { parseGroups, type GraphGroup } from './groups';

export const CANVAS_KIND = 'ledgerscope.canvas';
export const CANVAS_VERSION = 1;

export interface SavedPosition {
  x: number;
  y: number;
  pinned?: boolean;
}

export interface CanvasDocument {
  kind: typeof CANVAS_KIND;
  version: typeof CANVAS_VERSION;
  chain_id: number;
  scope: GraphScope;
  roots: GraphRoot[];
  selection: BlockSpan;
  ranges: BlockSpan[];
  filters: GraphFilters;
  label_mode: LabelMode;
  show_flow: boolean;
  removed: string[];
  hidden_txs: string[];
  groups: GraphGroup[];
  dimmed: string[];
  annotations: Annotation[];
  positions: Record<string, SavedPosition>;
  graph: GraphResponse | null;
}

export interface CanvasState {
  chainId: number;
  scope: GraphScope;
  roots: GraphRoot[];
  selection: BlockSpan;
  ranges: BlockSpan[];
  filters: GraphFilters;
  labelMode: LabelMode;
  showFlow: boolean;
  removed: Set<string>;
  hiddenTxs: Set<string>;
  groups: GraphGroup[];
  dimmed: Set<string>;
  annotations: Annotation[];
  positions: Map<string, NodePosition>;
  graph: GraphResponse | null;
}

function round(value: number): number {
  return Math.round(value * 10) / 10;
}

export function toDocument(state: CanvasState, withGraph = true): CanvasDocument {
  const positions: Record<string, SavedPosition> = {};
  for (const [id, position] of state.positions) {
    if (!Number.isFinite(position.x) || !Number.isFinite(position.y)) continue;
    const pinned = position.fx !== null && position.fx !== undefined;
    positions[id] = pinned ? { x: round(position.x), y: round(position.y), pinned } : { x: round(position.x), y: round(position.y) };
  }

  return {
    kind: CANVAS_KIND,
    version: CANVAS_VERSION,
    chain_id: state.chainId,
    scope: state.scope,
    roots: state.roots,
    selection: state.selection,
    ranges: state.ranges,
    filters: state.filters,
    label_mode: state.labelMode,
    show_flow: state.showFlow,
    removed: [...state.removed],
    hidden_txs: [...state.hiddenTxs],
    groups: state.groups,
    dimmed: [...state.dimmed],
    annotations: state.annotations,
    positions,
    graph: withGraph ? state.graph : null,
  };
}

export function fingerprint(state: CanvasState): string {
  return JSON.stringify({
    chain: state.chainId,
    scope: state.scope,
    roots: state.roots,
    selection: state.selection,
    ranges: state.ranges,
    filters: state.filters,
    label: state.labelMode,
    flow: state.showFlow,
    removed: [...state.removed].sort(),
    hiddenTxs: [...state.hiddenTxs].sort(),
    groups: state.groups,
    dimmed: [...state.dimmed].sort(),
    annotations: state.annotations,
  });
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isRoot(value: unknown): value is GraphRoot {
  return isObject(value) && typeof value.address === 'string' && typeof value.depth === 'number';
}

function isSpan(value: unknown): value is BlockSpan {
  return isObject(value) && Number.isFinite(value.from) && Number.isFinite(value.to);
}

function isGraph(value: unknown): value is GraphResponse {
  return isObject(value) && Array.isArray(value.nodes) && Array.isArray(value.edges);
}

export function parseDocument(raw: unknown): CanvasDocument {
  if (!isObject(raw) || raw.kind !== CANVAS_KIND) {
    throw new Error('This canvas was not saved by Ledgerscope.');
  }
  if (raw.version !== CANVAS_VERSION) {
    throw new Error(`This canvas has format version ${String(raw.version)}, this build reads ${CANVAS_VERSION}.`);
  }
  if (!Number.isFinite(raw.chain_id) || !Array.isArray(raw.roots) || !raw.roots.every(isRoot) || !isSpan(raw.selection)) {
    throw new Error('This canvas is damaged: chain, roots or block range are missing.');
  }

  const positions: Record<string, SavedPosition> = {};
  if (isObject(raw.positions)) {
    for (const [id, position] of Object.entries(raw.positions)) {
      if (isObject(position) && Number.isFinite(position.x) && Number.isFinite(position.y)) {
        positions[id] = { x: Number(position.x), y: Number(position.y), pinned: position.pinned === true };
      }
    }
  }

  return {
    kind: CANVAS_KIND,
    version: CANVAS_VERSION,
    chain_id: Number(raw.chain_id),
    scope: raw.scope === 'blocks' || raw.scope === 'taint' ? raw.scope : 'roots',
    roots: raw.roots,
    selection: raw.selection,
    ranges: Array.isArray(raw.ranges) && raw.ranges.length > 0 && raw.ranges.every(isSpan) ? raw.ranges : [raw.selection],
    filters: isObject(raw.filters) ? { ...DEFAULT_FILTERS, ...(raw.filters as Partial<GraphFilters>) } : DEFAULT_FILTERS,
    label_mode: (typeof raw.label_mode === 'string' ? raw.label_mode : 'auto') as LabelMode,
    show_flow: raw.show_flow !== false,
    removed: Array.isArray(raw.removed) ? raw.removed.filter((id): id is string => typeof id === 'string') : [],
    hidden_txs: Array.isArray(raw.hidden_txs) ? raw.hidden_txs.filter((hash): hash is string => typeof hash === 'string') : [],
    groups: parseGroups(raw.groups),
    dimmed: Array.isArray(raw.dimmed) ? raw.dimmed.filter((id): id is string => typeof id === 'string') : [],
    annotations: Array.isArray(raw.annotations) ? (raw.annotations as Annotation[]) : [],
    positions,
    graph: isGraph(raw.graph) ? raw.graph : null,
  };
}

export function restoredPositions(document: CanvasDocument): Map<string, NodePosition> {
  const out = new Map<string, NodePosition>();
  for (const [id, position] of Object.entries(document.positions)) {
    out.set(id, {
      x: position.x,
      y: position.y,
      vx: 0,
      vy: 0,
      fx: position.pinned ? position.x : null,
      fy: position.pinned ? position.y : null,
    });
  }
  return out;
}

export function documentBytes(document: CanvasDocument): number {
  return new Blob([JSON.stringify(document)]).size;
}
