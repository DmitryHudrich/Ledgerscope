import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';

import { RpcConfirmationRequired, fetchCoverage, fetchGraph } from './api/client';
import { demoGraph } from './api/demo';
import type {
  BlockRangeSpan,
  CoverageResponse,
  GraphResponse,
  GraphRoot,
  RpcConfirmation,
} from './api/types';
import { DetailsDrawer } from './components/DetailsDrawer';
import { IconFilter } from './components/Icons';
import { InvestigationPanel, type WorkspacePanel } from './components/SidePanel';
import type { BlockBounds } from './components/Timeline';
import { ToolRail } from './components/ToolRail';
import { TopBar } from './components/TopBar';
import { SHEET_HEIGHT, TxSheet } from './components/TxSheet';
import { getFilterActivity } from './components/filterActivity';
import { RpcConfirm } from './components/RpcConfirm';
import { Button, Dot, Kbd, Panel } from './components/ui';
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
const INITIAL_SELECTION: BlockBounds = { from: 21_000_000, to: 21_000_010 };
const FALLBACK_LIMITS = { maxDepth: 5, maxRoots: 64, maxBlocks: 100_000 };
const HEAD_WINDOW = 100;

type TemporaryPanel = WorkspacePanel | 'advanced';

function storedSelection(coverage: CoverageResponse, maxBlocks: number): BlockBounds | null {
  const widest = coverage.ranges.reduce<BlockRangeSpan | null>(
    (best, range) => (range.block_count > (best?.block_count ?? 0) ? range : best),
    null,
  );
  if (widest) {
    return {
      from: widest.from_block,
      to: Math.min(widest.to_block, widest.from_block + maxBlocks - 1),
    };
  }
  if (coverage.chain_head === null) return null;
  return {
    from: Math.max(coverage.chain_head - Math.min(10, HEAD_WINDOW), 0),
    to: coverage.chain_head,
  };
}

export default function App() {
  const [theme, setTheme] = useTheme();
  const [palette, setPalette] = useState<VizPalette>(() => readPalette());
  const [roots, setRoots] = useState<GraphRoot[]>(INITIAL_ROOTS);
  const [selection, setSelection] = useState<BlockBounds>(INITIAL_SELECTION);
  const [demo, setDemo] = useState(true);

  const [response, setResponse] = useState<GraphResponse | null>(null);
  const [coverage, setCoverage] = useState<CoverageResponse | null>(null);
  const [coverageLoaded, setCoverageLoaded] = useState(false);
  const [primed, setPrimed] = useState(false);
  const [pending, setPending] = useState<RpcConfirmation | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [buildingFromId, setBuildingFromId] = useState<string | null>(null);
  const [buildFeedback, setBuildFeedback] = useState<{
    nodeId: string;
    kind: 'success' | 'info' | 'error';
    message: string;
  } | null>(null);
  const [incrementalLayoutVersion, setIncrementalLayoutVersion] = useState(0);

  const [filterState, setFilterState] = useState<GraphFilters>(DEFAULT_FILTERS);
  const [search, setSearch] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [labelMode, setLabelMode] = useState<LabelMode>('auto');
  const [showFlow, setShowFlow] = useState(true);
  const [frozen, setFrozen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [sheetScope, setSheetScope] = useState<string | null>(null);
  const [temporaryPanel, setTemporaryPanel] = useState<TemporaryPanel | null>(null);

  const graph = useRef<GraphHandle | null>(null);
  const modelRef = useRef<GraphModel>(EMPTY_MODEL);
  const requestId = useRef(0);
  const snapped = useRef(false);
  const buildingFrom = useRef(false);

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
    const previous = modelRef.current.nodes.length > 0 ? positionsOf(modelRef.current) : undefined;
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

  const filterActivity = useMemo(
    () => getFilterActivity(filterState, search),
    [filterState, search],
  );
  const resetFilters = useCallback(() => {
    setFilterState(DEFAULT_FILTERS);
    setSearch('');
  }, []);

  const run = useCallback(
    async (confirmRpc = false, requestedRoots = roots): Promise<GraphResponse | null> => {
      if (requestedRoots.length === 0) return null;
      const id = (requestId.current += 1);
      setError(null);
      setPending(null);
      setBuildFeedback(null);
      setTemporaryPanel(null);
      setLoading(true);

      const query = {
        roots: requestedRoots,
        from_block: selection.from,
        to_block: selection.to,
        confirm_rpc: confirmRpc,
      };
      try {
        const payload = demo ? demoGraph(query) : await fetchGraph(query);
        if (requestId.current !== id) return null;
        setResponse(payload);
        return payload;
      } catch (cause) {
        if (requestId.current !== id) return null;
        if (cause instanceof RpcConfirmationRequired) {
          setPending(cause.confirmation);
        } else {
          setError(cause instanceof Error ? cause.message : String(cause));
          setResponse(null);
        }
        return null;
      } finally {
        if (requestId.current === id) setLoading(false);
      }
    },
    [demo, roots, selection],
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
    const next = coverage ? storedSelection(coverage, coverage.limits.max_blocks) : null;
    if (next) setSelection(next);
    setPrimed(true);
  }, [coverageLoaded, coverage]);

  useEffect(() => {
    if (primed) void run();
    // Initial build waits until coverage has supplied the most useful range.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [primed]);

  const submitFromTopBar = useCallback(
    (address: string | null) => {
      const nextRoots =
        address && !roots.some((root) => root.address === address)
          ? [...roots, { address, depth: 1 }]
          : roots;
      if (nextRoots !== roots) setRoots(nextRoots);
      void run(false, nextRoots);
    },
    [roots, run],
  );

  const removeRoot = useCallback((address: string) => {
    setRoots((current) => current.filter((root) => root.address !== address));
  }, []);
  const changeDepth = useCallback((address: string, depth: number) => {
    setRoots((current) =>
      current.map((root) => (root.address === address ? { ...root, depth } : root)),
    );
  }, []);

  const buildFromHere = useCallback(
    async (nodeId: string) => {
      if (demo || loading || buildingFrom.current) return;
      if (roots.some((root) => root.address === nodeId)) {
        setBuildFeedback({ nodeId, kind: 'info', message: 'Already an investigation root.' });
        return;
      }
      if (roots.length >= limits.maxRoots) {
        setBuildFeedback({
          nodeId,
          kind: 'error',
          message: `At most ${limits.maxRoots} investigation roots are allowed.`,
        });
        return;
      }

      const nextRoots = [...roots, { address: nodeId, depth: 1 }];
      buildingFrom.current = true;
      setBuildingFromId(nodeId);
      setRoots(nextRoots);
      const payload = await run(false, nextRoots);
      buildingFrom.current = false;
      setBuildingFromId(null);
      if (payload) {
        setSelectedId(nodeId);
        setIncrementalLayoutVersion((version) => version + 1);
        setBuildFeedback({ nodeId, kind: 'success', message: 'Added as an investigation root.' });
      }
    },
    [demo, limits.maxRoots, loading, roots, run],
  );

  const select = useCallback((id: string | null) => {
    setTemporaryPanel(null);
    if (id !== null) setSelectedId(id);
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable;
      if (event.key === 'Escape') {
        if (temporaryPanel) setTemporaryPanel(null);
        else if (pending) setPending(null);
        return;
      }
      if (typing) return;
      if (event.key === '/') {
        event.preventDefault();
        setTemporaryPanel('filters');
        window.requestAnimationFrame(() => document.getElementById('f-search')?.focus());
      } else if (event.key === 'f') {
        graph.current?.fit();
      } else if (event.key === ' ') {
        event.preventDefault();
        setFrozen((value) => !value);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [pending, temporaryPanel]);

  const selectedNode = selectedId ? (model.byId.get(selectedId) ?? null) : null;
  const hasGraph = model.nodes.length > 0;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <TopBar
        roots={roots}
        maxRoots={limits.maxRoots}
        selection={selection}
        onSelectionChange={setSelection}
        onSubmit={submitFromTopBar}
        loading={loading || buildingFromId !== null}
        demo={demo}
        onDemoChange={setDemo}
        advancedOpen={temporaryPanel === 'advanced'}
        onAdvancedOpenChange={(open) => setTemporaryPanel(open ? 'advanced' : null)}
        theme={theme}
        onThemeChange={setTheme}
      />

      <main
        className="relative min-h-0 flex-1 overflow-hidden bg-plane [--inspector-w:0px] [--investigation-w:48px] [--sheet-h:40px]"
        style={
          {
            '--sheet-h': hasGraph && sheetOpen ? SHEET_HEIGHT.expanded : SHEET_HEIGHT.collapsed,
            '--investigation-w': '48px',
            '--inspector-w': selectedNode ? '320px' : '0px',
            '--left-flyout-top': demo ? '68px' : '12px',
            '--filter-indicator-left':
              temporaryPanel !== null && temporaryPanel !== 'advanced' ? '332px' : '60px',
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
          incrementalLayoutVersion={incrementalLayoutVersion}
          onSelect={select}
          handle={graph}
        />

        <div
          className="pointer-events-none absolute inset-0 bg-[radial-gradient(120%_90%_at_50%_40%,transparent_55%,color-mix(in_srgb,var(--plane)_70%,transparent)_100%)]"
          aria-hidden="true"
        />

        <InvestigationPanel
          model={model}
          roots={roots}
          maxDepth={limits.maxDepth}
          filters={filterState}
          onFiltersChange={setFilterState}
          search={search}
          onSearchChange={setSearch}
          matchCount={searchMatches.size}
          activePanel={temporaryPanel === 'advanced' ? null : temporaryPanel}
          onPanelChange={setTemporaryPanel}
          activeFilterCount={filterActivity.activeFilterCount}
          onResetFilters={resetFilters}
          onRootRemove={removeRoot}
          onRootDepthChange={changeDepth}
          onSelect={(id) => {
            setSelectedId(id);
            setTemporaryPanel(null);
          }}
          onCenter={(id) => graph.current?.centerOn(id)}
        />

        {demo && (
          <Panel
            className="absolute left-[calc(var(--investigation-w)+12px)] top-3 z-10 flex items-center gap-2 px-2 py-1.5 text-xs text-text-secondary"
            role="status"
          >
            <Dot tone="focus" />
            <strong className="font-semibold text-text-primary">Demo mode</strong>
            <span className="max-[720px]:hidden">Showing generated sample data.</span>
            <Button variant="ghost" className="h-7 px-2" onClick={() => setDemo(false)}>
              Use live data
            </Button>
          </Panel>
        )}

        {filterActivity.hasActiveFilters && temporaryPanel !== 'filters' && (
          <Panel
            className="absolute left-[var(--filter-indicator-left)] top-[var(--left-flyout-top)] z-10 flex items-center gap-1 p-1 text-xs text-text-secondary transition-[left,top] duration-150"
            role="status"
            aria-label={`${filterActivity.activeFilterCount} active ${filterActivity.activeFilterCount === 1 ? 'filter' : 'filters'}`}
          >
            <Button variant="ghost" className="h-7 px-2" onClick={() => setTemporaryPanel('filters')}>
              <IconFilter size={13} /> {filterActivity.summary}
            </Button>
            <span className="h-4 w-px bg-hairline" aria-hidden="true" />
            <Button variant="ghost" className="h-7 px-2" onClick={resetFilters}>Reset</Button>
          </Panel>
        )}

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
            onSelect={select}
            onBuildFromHere={() => void buildFromHere(selectedNode.id)}
            buildingFromHere={buildingFromId === selectedNode.id}
            buildFromHereDisabled={
              demo || loading || roots.some((root) => root.address === selectedNode.id)
            }
            buildFromHereHint={
              demo
                ? 'Available for live data.'
                : roots.some((root) => root.address === selectedNode.id)
                  ? 'Already an investigation root.'
                  : null
            }
            buildFeedback={
              buildFeedback?.nodeId === selectedNode.id
                ? { kind: buildFeedback.kind, message: buildFeedback.message }
                : null
            }
            onCenter={(id) => graph.current?.centerOn(id)}
            onShowTransactions={() => {
              setTemporaryPanel(null);
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
            onOpenChange={(open) => {
              if (open) setTemporaryPanel(null);
              setSheetOpen(open);
            }}
            onScopeClear={() => setSheetScope(null)}
            onSelect={(id) => {
              setTemporaryPanel(null);
              setSelectedId(id);
              graph.current?.centerOn(id);
            }}
          />
        )}

        {pending && (
          <RpcConfirm
            confirmation={pending}
            onConfirm={() => void run(true).then(refreshCoverage)}
            onCancel={() => setPending(null)}
          />
        )}

        {loading && (
          <div className="pointer-events-none absolute bottom-[var(--sheet-h)] left-[var(--investigation-w)] right-[var(--inspector-w)] top-0 z-11 grid place-content-center justify-items-center p-6 text-center">
            <Panel className="pointer-events-auto grid max-w-[440px] justify-items-center gap-2 px-6 py-[22px]">
              <div className="size-[22px] animate-spin-exact rounded-full border-2 border-hairline-strong border-t-accent" aria-hidden="true" />
              <h2 className="m-0 text-base font-semibold">Building investigation graph</h2>
              <p className="m-0 text-[13px] text-text-secondary">
                Stored blocks answer immediately. Missing live blocks can take longer over JSON-RPC.
              </p>
            </Panel>
          </div>
        )}

        {!loading && !pending && error && (
          <div className="pointer-events-none absolute bottom-[var(--sheet-h)] left-[var(--investigation-w)] right-[var(--inspector-w)] top-0 z-11 grid place-content-center p-6 text-center">
            <Panel className="pointer-events-auto grid max-w-[440px] justify-items-center gap-2 px-6 py-[22px]">
              <h2 className="m-0 text-base font-semibold text-critical">Could not load the graph</h2>
              <p className="m-0 text-[13px] text-text-secondary">{error}</p>
              <div className="flex gap-2">
                <Button variant="primary" onClick={() => void run()}>Retry</Button>
                <Button onClick={() => { setError(null); setDemo(true); setResponse(demoGraph({ roots, from_block: selection.from, to_block: selection.to })); }}>
                  Use demo data
                </Button>
              </div>
            </Panel>
          </div>
        )}

        {!loading && !error && !pending && !hasGraph && (
          <div className="pointer-events-none absolute bottom-[var(--sheet-h)] left-[var(--investigation-w)] right-0 top-0 z-11 grid place-content-center p-6 text-center">
            <div className="grid max-w-[420px] justify-items-center gap-2 px-6 py-[22px]">
              <h2 className="m-0 text-base font-semibold">Start an investigation</h2>
              <p className="m-0 text-[13px] text-text-secondary">Paste an Ethereum address above to add it to the canvas.</p>
              <p className="m-0 text-xs text-text-muted">Explore transfers, counterparties and transaction paths.</p>
              <div className="mt-2 text-[11px] text-text-muted"><Kbd>/</Kbd> filters · <Kbd>f</Kbd> fit · <Kbd>space</Kbd> freeze</div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
