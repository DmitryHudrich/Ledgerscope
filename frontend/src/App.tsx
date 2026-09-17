import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';

import { RpcConfirmationRequired, fetchCoverage, fetchGraph, fetchHistogram } from './api/client';
import type {
  BlockRangeSpan,
  CoverageResponse,
  GraphResponse,
  GraphRoot,
  HistogramResponse,
  RpcConfirmation,
} from './api/types';
import { DetailsDrawer } from './components/DetailsDrawer';
import { RootsBar } from './components/RootsBar';
import { RpcConfirm } from './components/RpcConfirm';
import { SidePanel } from './components/SidePanel';
import { Timeline, type BlockBounds } from './components/Timeline';
import { ToolRail } from './components/ToolRail';
import { TopBar } from './components/TopBar';
import { SHEET_HEIGHT, TxSheet } from './components/TxSheet';
import { GraphCanvas, type GraphHandle } from './graph/GraphCanvas';
import type { LabelMode } from './graph/draw';
import {
  DEFAULT_FILTERS,
  EMPTY_MODEL,
  buildGraph,
  positionsOf,
  type GraphFilters,
  type GraphModel,
} from './graph/model';
import { readPalette, useTheme, type VizPalette } from './lib/theme';

const INITIAL_ROOTS: GraphRoot[] = [
  { address: '0xd8da6bf26964af9d7eed9e03e53415d37aa96045', depth: 1 },
];

const INITIAL_BOUNDS: BlockBounds = { from: 21_000_000, to: 21_000_100 };
const INITIAL_SELECTION: BlockBounds = { from: 21_000_000, to: 21_000_010 };

const FALLBACK_LIMITS = { maxDepth: 5, maxRoots: 64, maxBlocks: 100_000 };

const HEAD_WINDOW = 100;

function storedDefaults(
  coverage: CoverageResponse,
  maxBlocks: number,
): { bounds: BlockBounds; selection: BlockBounds } | null {
  const widest = coverage.ranges.reduce<BlockRangeSpan | null>(
    (best, range) => (range.block_count > (best?.block_count ?? 0) ? range : best),
    null,
  );

  if (widest && coverage.lowest_block !== null && coverage.highest_block !== null) {
    return {
      bounds: { from: coverage.lowest_block, to: coverage.highest_block },
      selection: {
        from: widest.from_block,
        to: Math.min(widest.to_block, widest.from_block + maxBlocks - 1),
      },
    };
  }

  if (coverage.chain_head === null) return null;

  const head = coverage.chain_head;
  return {
    bounds: { from: Math.max(head - HEAD_WINDOW, 0), to: head },
    selection: { from: Math.max(head - 10, 0), to: head },
  };
}

export default function App() {
  const [theme, setTheme] = useTheme();
  const [palette, setPalette] = useState<VizPalette>(() => readPalette());

  const [roots, setRoots] = useState<GraphRoot[]>(INITIAL_ROOTS);
  const [bounds, setBounds] = useState<BlockBounds>(INITIAL_BOUNDS);
  const [selection, setSelection] = useState<BlockBounds>(INITIAL_SELECTION);

  const [response, setResponse] = useState<GraphResponse | null>(null);
  const [coverage, setCoverage] = useState<CoverageResponse | null>(null);
  const [histogram, setHistogram] = useState<HistogramResponse | null>(null);
  const [coverageLoaded, setCoverageLoaded] = useState(false);
  const [primed, setPrimed] = useState(false);
  const [pending, setPending] = useState<RpcConfirmation | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [filterState, setFilterState] = useState<GraphFilters>(DEFAULT_FILTERS);
  const [search, setSearch] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [labelMode, setLabelMode] = useState<LabelMode>('auto');
  const [showFlow, setShowFlow] = useState(true);
  const [frozen, setFrozen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [sheetScope, setSheetScope] = useState<string | null>(null);

  const graph = useRef<GraphHandle | null>(null);
  const modelRef = useRef<GraphModel>(EMPTY_MODEL);
  const requestId = useRef(0);
  const snapped = useRef(false);

  const limits = useMemo(
    () => ({
      maxDepth: coverage?.limits.max_depth ?? FALLBACK_LIMITS.maxDepth,
      maxRoots: coverage?.limits.max_roots ?? FALLBACK_LIMITS.maxRoots,
      maxBlocks: coverage?.limits.max_blocks ?? FALLBACK_LIMITS.maxBlocks,
    }),
    [coverage],
  );

  useEffect(() => {
    setPalette(readPalette());
  }, [theme]);

  const filters = useMemo<GraphFilters>(() => ({ ...filterState, focus: '' }), [filterState]);

  const model = useMemo(() => {
    if (!response) return EMPTY_MODEL;
    const previous =
      modelRef.current.nodes.length > 0 ? positionsOf(modelRef.current) : undefined;
    return buildGraph(response, filters, previous);
  }, [response, filters]);

  useEffect(() => {
    modelRef.current = model;
  }, [model]);

  useEffect(() => {
    if (selectedId && !model.byId.has(selectedId)) setSelectedId(null);
    if (sheetScope && !model.byId.has(sheetScope)) setSheetScope(null);
  }, [model, selectedId, sheetScope]);

  const searchMatches = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (needle.length < 2) return new Set<string>();
    const matches = new Set<string>();
    for (const node of model.nodes) {
      if (node.id.includes(needle)) matches.add(node.id);
    }
    return matches;
  }, [search, model]);

  const run = useCallback(
    async (confirmRpc = false) => {
      if (roots.length === 0) return;
      const id = (requestId.current += 1);

      setError(null);
      setPending(null);

      setLoading(true);
      try {
        const payload = await fetchGraph({
          roots,
          from_block: selection.from,
          to_block: selection.to,
          confirm_rpc: confirmRpc,
        });
        if (requestId.current !== id) return;
        setResponse(payload);
      } catch (cause) {
        if (requestId.current !== id) return;
        if (cause instanceof RpcConfirmationRequired) {
          setPending(cause.confirmation);
        } else {
          setError(cause instanceof Error ? cause.message : String(cause));
          setResponse(null);
        }
      } finally {
        if (requestId.current === id) setLoading(false);
      }
    },
    [roots, selection],
  );

  const refreshCoverage = useCallback(async () => {
    try {
      setCoverage(await fetchCoverage());
    } catch {
      setCoverage(null);
    } finally {
      setCoverageLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refreshCoverage();
  }, [refreshCoverage]);

  useEffect(() => {
    if (!coverageLoaded || snapped.current) return;
    snapped.current = true;

    const defaults = coverage ? storedDefaults(coverage, coverage.limits.max_blocks) : null;
    if (defaults) {
      setBounds(defaults.bounds);
      setSelection(defaults.selection);
    }

    setPrimed(true);
  }, [coverageLoaded, coverage]);

  useEffect(() => {
    const aborter = new AbortController();
    fetchHistogram({ from_block: bounds.from, to_block: bounds.to }, aborter.signal)
      .then(setHistogram)
      .catch(() => setHistogram(null));

    return () => aborter.abort();
  }, [bounds]);

  useEffect(() => {
    if (primed) void run();
    // the first build waits for the block range the store suggests
  }, [primed]);

  const addRoot = useCallback(
    (address: string) => {
      setRoots((current) =>
        current.some((root) => root.address === address)
          ? current
          : [...current, { address, depth: 1 }],
      );
    },
    [],
  );

  const removeRoot = useCallback((address: string) => {
    setRoots((current) => current.filter((root) => root.address !== address));
  }, []);

  const changeDepth = useCallback((address: string, depth: number) => {
    setRoots((current) =>
      current.map((root) => (root.address === address ? { ...root, depth } : root)),
    );
  }, []);

  const select = useCallback((id: string | null) => {
    setSelectedId(id);
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable;

      if (event.key === 'Escape') {
        if (typing) return;
        if (pending) setPending(null);
        else if (selectedId) setSelectedId(null);
        else if (sheetOpen) setSheetOpen(false);
        return;
      }
      if (typing) return;
      if (event.key === '/') {
        event.preventDefault();
        document.getElementById('f-search')?.focus();
      } else if (event.key === 'f') {
        graph.current?.fit();
      } else if (event.key === ' ') {
        event.preventDefault();
        setFrozen((value) => !value);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [selectedId, sheetOpen, pending]);

  const selectedNode = selectedId ? (model.byId.get(selectedId) ?? null) : null;
  const hasGraph = model.nodes.length > 0;

  return (
    <div className="app">
      <TopBar theme={theme} onThemeChange={setTheme} />

      <RootsBar
        roots={roots}
        maxDepth={limits.maxDepth}
        maxRoots={limits.maxRoots}
        loading={loading}
        onAdd={addRoot}
        onRemove={removeRoot}
        onDepthChange={changeDepth}
        onSubmit={() => void run()}
      />

      <main
        className="stage"
        style={
          {
            '--sheet-h': `${
              hasGraph && sheetOpen ? SHEET_HEIGHT.expanded : SHEET_HEIGHT.collapsed
            }px`,
          } as CSSProperties
        }
      >
        <GraphCanvas
          model={model}
          palette={palette}
          selectedId={selectedId}
          searchMatches={searchMatches}
          labelMode={labelMode}
          showGrid
          showFlow={showFlow}
          frozen={frozen}
          onSelect={select}
          handle={graph}
        />

        {hasGraph && (
          <SidePanel
            model={model}
            filters={filterState}
            onFiltersChange={setFilterState}
            search={search}
            onSearchChange={setSearch}
            matchCount={searchMatches.size}
          />
        )}

        <Timeline
          bounds={bounds}
          selection={selection}
          coverage={coverage}
          histogram={histogram}
          maxBlocks={limits.maxBlocks}
          onBoundsChange={setBounds}
          onSelectionChange={setSelection}
        />

        <ToolRail
          onZoomIn={() => graph.current?.zoomBy(1.35)}
          onZoomOut={() => graph.current?.zoomBy(1 / 1.35)}
          onFit={() => graph.current?.fit()}
          frozen={frozen}
          onFrozenChange={setFrozen}
          showFlow={showFlow}
          onShowFlowChange={setShowFlow}
          labelMode={labelMode}
          onLabelModeChange={setLabelMode}
          onUnpin={() => graph.current?.unpinAll()}
          onExport={() => graph.current?.exportPng('ledgerscope.png')}
          disabled={!hasGraph}
        />

        {selectedNode && (
          <DetailsDrawer
            node={selectedNode}
            model={model}
            rooted={roots.some((root) => root.address === selectedNode.id)}
            onSelect={select}
            onCenter={(id) => graph.current?.centerOn(id)}
            onExpand={() => {
              addRoot(selectedNode.id);
              setSelectedId(null);
            }}
            onShowTransactions={() => {
              setSheetScope(selectedNode.id);
              setSheetOpen(true);
            }}
            onClose={() => setSelectedId(null)}
          />
        )}

        {hasGraph && (
          <TxSheet
            model={model}
            scope={sheetScope}
            open={sheetOpen}
            onOpenChange={setSheetOpen}
            onScopeClear={() => setSheetScope(null)}
            onSelect={(id) => {
              setSelectedId(id);
              graph.current?.centerOn(id);
            }}
          />
        )}

        {pending && (
          <RpcConfirm
            confirmation={pending}
            onConfirm={() => {
              void run(true).then(refreshCoverage);
            }}
            onCancel={() => setPending(null)}
          />
        )}

        {loading && (
          <div className="state">
            <div className="panel state-card">
              <div className="spinner" aria-hidden="true" />
              <h2>Walking the graph</h2>
              <p>
                Blocks already in clickhouse answer right away. Blocks the node still has to hand
                over take a while.
              </p>
            </div>
          </div>
        )}

        {!loading && !pending && error && (
          <div className="state state-error">
            <div className="panel state-card">
              <h2>Could not load the graph</h2>
              <p>{error}</p>
              <div className="query">
                <button type="button" className="btn btn-primary" onClick={() => void run()}>
                  Retry
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    setError(null);
                    void refreshCoverage();
                  }}
                >
                  Dismiss
                </button>
              </div>
            </div>
          </div>
        )}

        {!loading && !error && !pending && !hasGraph && (
          <div className="state">
            <div className="panel state-card">
              <h2>No graph yet</h2>
              <p>
                Add an address, pick its depth and a block range, then press{' '}
                <span className="kbd">Build graph</span>. Press <span className="kbd">/</span> to
                search, <span className="kbd">f</span> to fit, <span className="kbd">space</span> to
                freeze the layout.
              </p>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
