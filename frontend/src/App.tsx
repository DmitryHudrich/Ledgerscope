import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';

import { fetchGraph } from './api/client';
import { demoGraph } from './api/demo';
import type { GraphResponse } from './api/types';
import { DetailsDrawer } from './components/DetailsDrawer';
import { SidePanel } from './components/SidePanel';
import { ToolRail } from './components/ToolRail';
import { TopBar, parseDraft, type QueryDraft } from './components/TopBar';
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

const INITIAL_DRAFT: QueryDraft = {

  wallet: '0xd8da6bf26964af9d7eed9e03e53415d37aa96045',
  from: '21000000',
  to: '21000010',
};

export default function App() {
  const [theme, setTheme] = useTheme();
  const [palette, setPalette] = useState<VizPalette>(() => readPalette());

  const [draft, setDraft] = useState<QueryDraft>(INITIAL_DRAFT);
  const [demo, setDemo] = useState(true);
  const [response, setResponse] = useState<GraphResponse | null>(null);
  const [activeWallet, setActiveWallet] = useState('');
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

  useEffect(() => {
    setPalette(readPalette());
  }, [theme]);

  const filters = useMemo<GraphFilters>(
    () => ({ ...filterState, focus: activeWallet }),
    [filterState, activeWallet],
  );

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

  const run = useCallback(async () => {
    const query = parseDraft(draft);
    if (!query) return;
    const id = (requestId.current += 1);

    setActiveWallet(query.wallet);
    setError(null);

    if (demo) {
      setResponse(demoGraph(query));
      setLoading(false);
      return;
    }

    setLoading(true);
    try {
      const payload = await fetchGraph(query);
      if (requestId.current !== id) return;
      setResponse(payload);
    } catch (cause) {
      if (requestId.current !== id) return;
      setError(cause instanceof Error ? cause.message : String(cause));
      setResponse(null);
    } finally {
      if (requestId.current === id) setLoading(false);
    }
  }, [draft, demo]);

  useEffect(() => {
    void run();
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
        if (selectedId) setSelectedId(null);
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
  }, [selectedId, sheetOpen]);

  const selectedNode = selectedId ? (model.byId.get(selectedId) ?? null) : null;
  const hasGraph = model.nodes.length > 0;

  return (
    <div className="app">
      <TopBar
        draft={draft}
        onDraftChange={setDraft}
        onSubmit={() => void run()}
        loading={loading}
        demo={demo}
        onDemoChange={setDemo}
        theme={theme}
        onThemeChange={setTheme}
      />

      {demo && (
        <div className="banner" role="status">
          <span className="dot dot-focus" aria-hidden="true" />
          Generated sample data — turn off <strong>Demo data</strong> and rebuild to query the
          backend at <code>/api/graph</code>.
        </div>
      )}

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
          onExport={() => graph.current?.exportPng(`ledgerscope-${activeWallet.slice(0, 10)}.png`)}
          disabled={!hasGraph}
        />

        {selectedNode && (
          <DetailsDrawer
            node={selectedNode}
            model={model}
            onSelect={select}
            onCenter={(id) => graph.current?.centerOn(id)}
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

        {loading && (
          <div className="state">
            <div className="panel state-card">
              <div className="spinner" aria-hidden="true" />
              <h2>Seeding blocks</h2>
              <p>
                The backend walks every block in the range over JSON-RPC before it answers. A wide
                range can take a while.
              </p>
            </div>
          </div>
        )}

        {!loading && error && (
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
                    setDemo(true);
                    setError(null);
                    setResponse(demoGraph(parseDraft(draft) ?? { wallet: '', from: 0, to: 1 }));
                  }}
                >
                  Use demo data
                </button>
              </div>
            </div>
          </div>
        )}

        {!loading && !error && !hasGraph && (
          <div className="state">
            <div className="panel state-card">
              <h2>No graph yet</h2>
              <p>
                Enter a wallet and a block range, then press <span className="kbd">Build graph</span>
                . Press <span className="kbd">/</span> to search, <span className="kbd">f</span> to
                fit, <span className="kbd">space</span> to freeze the layout.
              </p>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
