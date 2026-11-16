import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';

import { ApiError, RpcConfirmationRequired, fetchChains, fetchCoverage, fetchGraph, fetchHead, fetchHistogram } from './api/client';
import { canvasApi, isUserLabelSource, labelApi, type CanvasInfo, type CanvasVersion, type Me, type OwnerRequest, type UserLabel } from './api/account';
import type {
  ChainInfo,
  CoverageResponse,
  GraphResponse,
  GraphRoot,
  GraphScope,
  HistogramResponse,
  RpcConfirmation,
} from './api/types';
import { DetailsDrawer } from './components/DetailsDrawer';
import { EdgeInspector, MultiSelectionInspector } from './components/ContextInspector';
import { IconFilter } from './components/Icons';
import { InvestigationPanel, type WorkspacePanel } from './components/SidePanel';
import { BlockRangePanel } from './components/BlockRangePanel';
import { Timeline, type BlockBounds } from './components/Timeline';
import { ToolRail } from './components/ToolRail';
import { TopBar } from './components/TopBar';
import { SHEET_HEIGHT, TxSheet } from './components/TxSheet';
import { getFilterActivity } from './components/filterActivity';
import { RpcConfirm } from './components/RpcConfirm';
import { ShortcutsHelp } from './components/ShortcutsHelp';
import { CanvasSearch } from './components/CanvasSearch';
import { MIN_QUERY, searchNodes } from './lib/nodeSearch';
import { formatCount } from './lib/format';
import { AccountMenu, type AccountAction } from './components/account/AccountMenu';
import { AuthDialog } from './components/account/AuthDialog';
import { CanvasControls, type SaveState } from './components/account/CanvasControls';
import { CanvasesDialog } from './components/account/CanvasesDialog';
import { InfoDialog } from './components/account/InfoDialog';
import { LabelsDialog } from './components/account/LabelsDialog';
import { RpcNodesDialog } from './components/account/RpcNodesDialog';
import { TlsDeanonDialog } from './components/account/TlsDeanonDialog';
import { NodeLabels } from './components/account/NodeLabels';
import { ProfileDialog } from './components/account/ProfileDialog';
import { SaveCanvasDialog } from './components/account/SaveCanvasDialog';
import { SnapshotDialog } from './components/account/SnapshotDialog';
import { ShareDialog } from './components/account/ShareDialog';
import { TeamsDialog } from './components/account/TeamsDialog';
import { errorText } from './components/Modal';
import { MonitorPanel } from './components/monitor/MonitorPanel';
import { BuildProgress } from './components/monitor/BuildProgress';
import { ScanProgress } from './components/monitor/ScanProgress';
import { Button, Kbd, Panel } from './components/ui';
import { GraphCanvas, type GraphHandle } from './graph/GraphCanvas';
import { DEFAULT_ANNOTATION_STYLE, type Annotation, type AnnotationStyle, type AnnotationTool } from './graph/annotations';
import type { LabelMode } from './graph/draw';
import { getNodeDisplayName, truncateCanvasLabel } from './graph/labels';
import {
  DEFAULT_FILTERS,
  EMPTY_MODEL,
  buildGraph,
  positionsOf,
  withoutNodes,
  type GraphFilters,
  type GraphModel,
  type NodePosition,
} from './graph/model';
import { clampRange, indexedWithin, sameRange, type BlockLimits, type BlockSpan } from './lib/blockRange';
import { readStoredChain, setActiveChain, storeChain } from './lib/chain';
import {
  documentBytes,
  fingerprint,
  parseDocument,
  restoredPositions,
  toDocument,
  type CanvasDocument,
  type CanvasState,
} from './lib/canvasDocument';
import { readLastCanvas, readSession, writeLastCanvas, writeSession } from './lib/session';
import { applySelection, component, grow, invert, neighborsOnly, shrink, type SelectMode } from './lib/selection';
import { useAccount } from './lib/useAccount';
import { useAddressForm } from './lib/useAddressForm';
import { useTlsLinks } from './lib/useTlsLinks';
import { addToGroup, hiddenSets, withoutTxs, type GraphGroup } from './lib/groups';
import { downloadJson, graphExport } from './lib/exportGraph';
import { focusSets, readFocus, writeFocus, type FocusSettings } from './lib/focus';
import { FocusControl } from './components/FocusControl';
import { GroupPicker } from './components/GroupPicker';
import { GroupsPanel } from './components/GroupsPanel';
import { IconEyeOff, IconFocus, IconSearch } from './components/Icons';
import { useMonitor, type Health } from './lib/useMonitor';
import { readPalette, useTheme, type VizPalette } from './lib/theme';

const INITIAL_SELECTION: BlockBounds = { from: 21_000_000, to: 21_000_010 };
const FALLBACK_LIMITS = { maxDepth: 5, maxRoots: 64, maxBlocks: 100_000 };
const HEAD_WINDOW = 100;
const ANNOTATIONS_KEY = 'ledgerscope:annotations:v1';
function readAnnotations(): Annotation[] {
  try { const stored = localStorage.getItem(ANNOTATIONS_KEY); return stored ? JSON.parse(stored) as Annotation[] : []; } catch { return []; }
}

type TemporaryPanel = WorkspacePanel | 'advanced';
type Dialog = 'auth' | 'profile' | 'teams' | 'canvases' | 'labels' | 'rpc' | 'tls' | 'info' | 'save' | 'snapshot' | 'share';

const SNAPSHOT_BUDGET = 7 * 1024 * 1024;
const AUTOSAVE_MS = 60_000;
const SESSION_WRITE_MS = 1_500;

const HEALTH_DOT: Record<Health, string> = {
  idle: 'bg-text-muted opacity-60',
  good: 'bg-good',
  degraded: 'bg-series-focus',
  failing: 'bg-critical',
  offline: 'bg-critical opacity-60',
};

const HEALTH_TITLE: Record<Health, string> = {
  idle: 'No RPC calls in the last minute',
  good: 'RPC nodes answer fine',
  degraded: 'Some RPC calls were retried in the last minute',
  failing: 'Many RPC calls fail',
  offline: 'The monitor is not connected to the server',
};
const LABEL_CHUNK = 200;

function urlParam(name: string): string | null {
  return new URLSearchParams(window.location.search).get(name);
}

function setUrlParam(name: string, value: string | null) {
  const url = new URL(window.location.href);
  for (const key of ['canvas', 'share']) url.searchParams.delete(key);
  if (value) url.searchParams.set(name, value);
  window.history.replaceState(null, '', url);
}

function withUserLabels(graph: GraphResponse, fresh: Map<string, UserLabel[]>): GraphResponse {
  return {
    ...graph,
    nodes: graph.nodes.map((node) => {
      const mine = fresh.get(node.address.toLowerCase());
      if (!mine) return node;
      const ordered = [...mine].sort((a, b) => Number(a.owner.kind === 'team') - Number(b.owner.kind === 'team'));
      return {
        ...node,
        labels: [
          ...ordered.map((label) => ({ value: label.value, source: label.owner.kind === 'team' ? `team:${label.owner.team_name ?? ''}` : 'you' })),
          ...node.labels.filter((label) => !isUserLabelSource(label.source)),
        ],
      };
    }),
  };
}

function withoutUserLabels(graph: GraphResponse): GraphResponse {
  return {
    ...graph,
    nodes: graph.nodes.map((node) => ({ ...node, labels: node.labels.filter((label) => !isUserLabelSource(label.source)) })),
  };
}

function chainOrder(defaultChain: number) {
  return (a: ChainInfo, b: ChainInfo) =>
    Number(b.chain_id === defaultChain) - Number(a.chain_id === defaultChain)
    || Number(b.rpc_nodes > 0) - Number(a.rpc_nodes > 0)
    || a.name.localeCompare(b.name);
}

function storedSelection(coverage: CoverageResponse, maxBlocks: number): BlockSpan[] | null {
  const indexed = indexedWithin(coverage.ranges, maxBlocks);
  if (indexed.length > 0) return indexed;
  if (coverage.chain_head === null) return null;
  return [{
    from: Math.max(coverage.chain_head - Math.min(10, HEAD_WINDOW), 0),
    to: coverage.chain_head,
  }];
}

export default function App() {
  const [theme, setTheme] = useTheme();
  const [palette, setPalette] = useState<VizPalette>(() => readPalette());
  const [roots, setRoots] = useState<GraphRoot[]>([]);
  const [scope, setScope] = useState<GraphScope>('roots');
  const [timelineOpen, setTimelineOpen] = useState(true);
  const [ranges, setRanges] = useState<BlockSpan[]>([INITIAL_SELECTION]);
  const [activeRange, setActiveRange] = useState(0);
  const selection = ranges[activeRange] ?? ranges[0] ?? INITIAL_SELECTION;
  const [chains, setChains] = useState<ChainInfo[]>([]);
  const [chainId, setChainId] = useState<number | null>(null);
  const [defaultChain, setDefaultChain] = useState<number | null>(null);
  const [chainsLoaded, setChainsLoaded] = useState(false);

  const [response, setResponse] = useState<GraphResponse | null>(null);
  const [coverage, setCoverage] = useState<CoverageResponse | null>(null);
  const [histogram, setHistogram] = useState<HistogramResponse | null>(null);
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
  const [restoredLayoutVersion, setRestoredLayoutVersion] = useState(0);

  const [filterState, setFilterState] = useState<GraphFilters>(DEFAULT_FILTERS);
  const [search, setSearch] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [selectedLinkId, setSelectedLinkId] = useState<string | null>(null);
  const [removedNodeIds, setRemovedNodeIds] = useState<Set<string>>(() => new Set());
  const [hiddenTxs, setHiddenTxs] = useState<Set<string>>(() => new Set());
  const [groups, setGroups] = useState<GraphGroup[]>([]);
  const [focus, setFocus] = useState<FocusSettings>(readFocus);
  const [dimmed, setDimmed] = useState<Set<string>>(() => new Set());
  const [annotations, setAnnotations] = useState<Annotation[]>(readAnnotations);
  const [annotationTool, setAnnotationTool] = useState<AnnotationTool>('select');
  const [annotationStyle, setAnnotationStyle] = useState<AnnotationStyle>(DEFAULT_ANNOTATION_STYLE);
  const [labelMode, setLabelMode] = useState<LabelMode>('auto');
  const [showFlow, setShowFlow] = useState(true);
  const [frozen, setFrozen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [sheetHeight, setSheetHeight] = useState<number>(SHEET_HEIGHT.default);
  const [sheetScope, setSheetScope] = useState<string | null>(null);
  const [sheetLinkScope, setSheetLinkScope] = useState<string | null>(null);
  const [temporaryPanel, setTemporaryPanel] = useState<TemporaryPanel | null>(null);

  const account = useAccount();
  useAddressForm();
  const tlsLinks = useTlsLinks(chainId, !!account.me);
  const monitor = useMonitor(chainsLoaded && chains.length > 0);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [canvas, setCanvas] = useState<CanvasInfo | null>(null);
  const [shareTarget, setShareTarget] = useState<CanvasInfo | null>(null);
  const [savedPrint, setSavedPrint] = useState<string | null>(null);
  const [openedVersion, setOpenedVersion] = useState<CanvasVersion | null>(null);
  const [lastAutosave, setLastAutosave] = useState<string | null>(null);
  const [autosaveFailed, setAutosaveFailed] = useState(false);
  const autosavedPrint = useRef<string | null>(null);
  const sessionRestored = useRef(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [publicView, setPublicView] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [monitorOpen, setMonitorOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [scanHidden, setScanHidden] = useState(false);
  const [canvasSearchOpen, setCanvasSearchOpen] = useState(false);
  const loadStartedAt = useRef(0);

  const graph = useRef<GraphHandle | null>(null);
  const annotationUndo = useRef<Annotation[][]>([]);
  const annotationRedo = useRef<Annotation[][]>([]);
  const annotationsRef = useRef(annotations);
  const modelRef = useRef<GraphModel>(EMPTY_MODEL);
  const requestId = useRef(0);
  const inFlight = useRef<AbortController | null>(null);
  const snapped = useRef(false);
  const buildingFrom = useRef(false);
  const restored = useRef<Map<string, NodePosition> | null>(null);
  const runAfterLoad = useRef(false);
  const openedFromUrl = useRef(false);

  const chain = useMemo(
    () => chains.find((candidate) => candidate.chain_id === chainId) ?? null,
    [chains, chainId],
  );

  const refreshChains = useCallback(async () => {
    try {
      const next = await fetchChains();
      const ordered = [...next.chains].sort(chainOrder(next.default_chain));
      setChains(ordered);
      setDefaultChain(next.default_chain);
      setChainId((current) => {
        if (current !== null && ordered.some((candidate) => candidate.chain_id === current)) return current;
        const stored = readStoredChain();
        const picked = ordered.find((candidate) => candidate.chain_id === stored)
          ?? ordered.find((candidate) => candidate.chain_id === next.default_chain)
          ?? ordered[0];
        setActiveChain(picked ?? null);
        return picked?.chain_id ?? null;
      });
    } catch {
      setChains([]);
    } finally {
      setChainsLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refreshChains();
  }, [refreshChains]);

  const limits = useMemo(
    () => ({
      maxDepth: coverage?.limits.max_depth ?? FALLBACK_LIMITS.maxDepth,
      maxRoots: coverage?.limits.max_roots ?? FALLBACK_LIMITS.maxRoots,
      maxBlocks: coverage?.limits.max_blocks ?? FALLBACK_LIMITS.maxBlocks,
    }),
    [coverage],
  );

  const blockLimits = useMemo<BlockLimits>(
    () => ({ head: coverage?.chain_head ?? null, maxBlocks: limits.maxBlocks }),
    [coverage?.chain_head, limits.maxBlocks],
  );


  useEffect(() => {
    setPalette(readPalette());
  }, [theme]);
  useEffect(() => { localStorage.setItem(ANNOTATIONS_KEY, JSON.stringify(annotations)); }, [annotations]);
  useEffect(() => { annotationsRef.current = annotations; }, [annotations]);

  const changeAnnotations = useCallback((next: Annotation[]) => {
    annotationUndo.current.push(annotationsRef.current);
    if (annotationUndo.current.length > 100) annotationUndo.current.shift();
    annotationRedo.current = [];
    annotationsRef.current = next;
    setAnnotations(next);
  }, []);

  const deferredFilters = useDeferredValue(filterState);
  const filters = useMemo<GraphFilters>(() => ({ ...deferredFilters, focus: '' }), [deferredFilters]);
  const hidden = useMemo(() => hiddenSets(removedNodeIds, hiddenTxs, groups), [groups, hiddenTxs, removedNodeIds]);
  const visibleResponse = useMemo(() => (response ? withoutTxs(response, hidden.txs) : null), [hidden.txs, response]);
  const rawModel = useMemo(() => {
    if (!visibleResponse) return EMPTY_MODEL;
    const previous = restored.current ?? (modelRef.current.nodes.length > 0 ? positionsOf(modelRef.current) : undefined);
    return buildGraph(visibleResponse, filters, previous);
  }, [visibleResponse, filters]);

  useEffect(() => {
    if (rawModel.nodes.length > 0) restored.current = null;
  }, [rawModel]);
  const model = useMemo(() => withoutNodes(rawModel, hidden.nodes), [rawModel, hidden.nodes]);
  const nodeLabels = useMemo(() => new Map(
    model.nodes.map((node) => [node.id, truncateCanvasLabel(getNodeDisplayName(node))] as const),
  ), [model]);

  useEffect(() => {
    modelRef.current = model;
  }, [model]);

  useEffect(() => {
    if (selectedId && !model.byId.has(selectedId)) setSelectedId(null);
    setSelectedIds((current) => {
      const next = new Set([...current].filter((id) => model.byId.has(id)));
      return next.size === current.size ? current : next;
    });
    if (selectedLinkId && !model.links.some((link) => link.id === selectedLinkId)) setSelectedLinkId(null);
    if (sheetScope && !model.byId.has(sheetScope)) setSheetScope(null);
  }, [model, selectedId, selectedLinkId, sheetScope]);

  const searchHits = useMemo(() => searchNodes(model, search), [search, model]);
  const searchMatches = useMemo(() => new Set(searchHits.map((hit) => hit.node.id)), [searchHits]);

  const focusResult = useMemo(() => focusSets(model, focus, {
    roots: new Set(roots.map((root) => root.address.toLowerCase())),
    selected: selectedIds,
    grouped: new Set(groups.filter((group) => !group.hidden).flatMap((group) => group.nodes)),
    searchMatches,
    dimmed,
  }), [dimmed, focus, groups, model, roots, searchMatches, selectedIds]);
  const fade = useMemo(
    () => (focusResult ? { nodes: focusResult.nodes, links: focusResult.links, opacity: focus.opacity, blur: focus.blur } : null),
    [focus.blur, focus.opacity, focusResult],
  );
  const changeFocus = useCallback((next: FocusSettings) => {
    setFocus(next);
    writeFocus(next);
  }, []);

  const dimNodes = useCallback((ids: string[], faded: boolean) => {
    setDimmed((current) => {
      const next = new Set(current);
      for (const id of ids) {
        if (faded) next.add(id);
        else next.delete(id);
      }
      return next;
    });
    if (faded) {
      setFocus((current) => {
        const next = { ...current, enabled: true, scope: 'chosen' as const };
        writeFocus(next);
        return next;
      });
    }
  }, []);

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
      if (scope === 'roots' && requestedRoots.length === 0) return null;
      const id = (requestId.current += 1);
      inFlight.current?.abort();
      const controller = new AbortController();
      inFlight.current = controller;
      setError(null);
      setPending(null);
      setBuildFeedback(null);
      setTemporaryPanel(null);
      setLoading(true);
      loadStartedAt.current = Date.now();

      const query = {
        chain: chainId ?? undefined,
        scope,
        roots: requestedRoots,
        from_block: selection.from,
        to_block: selection.to,
        ranges: ranges.map((range) => ({
          from_block: range.from,
          to_block: range.to,
          source: range.source ?? 'auto',
          exact: range.exact ?? false,
        })),
        confirm_rpc: confirmRpc,
      };
      try {
        const payload = await fetchGraph(query, controller.signal);
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
        if (inFlight.current === controller) inFlight.current = null;
      }
    },
    [chainId, roots, ranges, activeRange, scope],
  );

  const refreshCoverage = useCallback(async () => {
    const chainParam = chainId ?? undefined;
    try {
      const next = await fetchCoverage(chainParam);
      setCoverage(next);
      const from = next.lowest_block ?? Math.max((next.chain_head ?? INITIAL_SELECTION.to) - HEAD_WINDOW, 0);
      const to = next.highest_block ?? next.chain_head ?? INITIAL_SELECTION.to;
      try {
        setHistogram(await fetchHistogram({ chain: chainParam, from_block: from, to_block: to, buckets: 80 }));
      } catch {
        setHistogram(null);
      }
    } catch {
      setCoverage(null);
      setHistogram(null);
    } finally {
      setCoverageLoaded(true);
    }
  }, [chainId]);

  const stopFetching = useCallback(() => {
    requestId.current += 1;
    inFlight.current?.abort();
    inFlight.current = null;
    setLoading(false);
    setNotice('Fetch stopped. Blocks fetched so far stay indexed.');
    void refreshCoverage();
  }, [refreshCoverage]);

  useEffect(() => {
    if (chainsLoaded) void refreshCoverage();
  }, [chainsLoaded, refreshCoverage]);

  const changeChain = useCallback((next: number) => {
    const picked = chains.find((candidate) => candidate.chain_id === next);
    if (!picked || picked.chain_id === chainId) return;
    storeChain(picked.chain_id);
    setActiveChain(picked);
    requestId.current += 1;
    inFlight.current?.abort();
    snapped.current = false;
    setChainId(picked.chain_id);
    setResponse(null);
    setPending(null);
    setError(null);
    setLoading(false);
    setCoverage(null);
    setHistogram(null);
    setCoverageLoaded(false);
    setPrimed(false);
    setSelectedId(null);
    setSelectedIds(new Set());
    setSelectedLinkId(null);
    setRemovedNodeIds(new Set());
    setHiddenTxs(new Set());
    setGroups([]);
    setDimmed(new Set());
  }, [chainId, chains]);

  useEffect(() => {
    if (!coverageLoaded || snapped.current) return;
    snapped.current = true;
    const next = coverage ? storedSelection(coverage, coverage.limits.max_blocks) : null;
    if (next) {
      setRanges(next);
      setActiveRange(next.length - 1);
    }
    setPrimed(true);
  }, [coverageLoaded, coverage]);

  useEffect(() => {
    setRanges((current) =>
      current.map((range) => {
        const next = clampRange(range, blockLimits);
        return sameRange(range, next) ? range : next;
      }),
    );
  }, [blockLimits]);

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
      if (loading || buildingFrom.current) return;
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
    [limits.maxRoots, loading, roots, run],
  );

  const selectMany = useCallback((ids: Set<string>, mode: SelectMode = 'replace') => {
    setTemporaryPanel(null);
    setSelectedLinkId(null);
    setSelectedIds((current) => {
      const next = applySelection(current, ids, mode, modelRef.current);
      setSelectedId(next.size === 1 ? [...next][0] : null);
      return next;
    });
  }, []);

  const select = useCallback((id: string | null, mode: SelectMode = 'replace') => {
    if (id === null) {
      setTemporaryPanel(null);
      setSelectedLinkId(null);
      setSelectedId(null);
      setSelectedIds(new Set());
      return;
    }
    selectMany(new Set([id]), mode);
  }, [selectMany]);

  const reshapeSelection = useCallback((reshape: (model: GraphModel, selected: Set<string>) => Set<string>) => {
    setSelectedIds((current) => {
      const next = reshape(modelRef.current, current);
      setSelectedId(next.size === 1 ? [...next][0] : null);
      return next;
    });
    setSelectedLinkId(null);
  }, []);

  const selectLink = useCallback((id: string | null) => {
    setTemporaryPanel(null);
    setSelectedIds(new Set());
    setSelectedId(null);
    setSelectedLinkId(id);
  }, []);

  const removeSelected = useCallback(() => {
    if (selectedIds.size === 0) return;
    setIncrementalLayoutVersion((version) => version + 1);
    setRemovedNodeIds((current) => new Set([...current, ...selectedIds]));
    setSelectedIds(new Set());
    setSelectedId(null);
  }, [selectedIds]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable;
      if (event.key === 'Escape') {
        if (event.defaultPrevented) return;
        if (shortcutsOpen) setShortcutsOpen(false);
        else if (monitorOpen) setMonitorOpen(false);
        else if (selectedIds.size > 0 || selectedLinkId) {
          setSelectedIds(new Set());
          setSelectedId(null);
          setSelectedLinkId(null);
        } else if (temporaryPanel) setTemporaryPanel(null);
        else if (pending) setPending(null);
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.code === 'KeyF') {
        event.preventDefault();
        setCanvasSearchOpen(true);
        return;
      }
      if ((event.key === 'Delete' || event.key === 'Backspace') && selectedIds.size > 0 && !typing) {
        event.preventDefault();
        removeSelected();
        return;
      }
      if (typing) return;
      if ((event.ctrlKey || event.metaKey) && event.code === 'KeyA') {
        event.preventDefault();
        reshapeSelection((model) => new Set(model.nodes.map((node) => node.id)));
        return;
      }
      if (!event.ctrlKey && !event.metaKey && !event.altKey) {
        const reshape = event.code === 'KeyE'
          ? event.shiftKey ? shrink : grow
          : event.code === 'KeyN' && !event.shiftKey
            ? neighborsOnly
            : event.code === 'KeyC' && !event.shiftKey
              ? component
              : event.code === 'KeyI' && !event.shiftKey
                ? invert
                : null;
        if (reshape) {
          event.preventDefault();
          reshapeSelection(reshape);
          return;
        }
        if (event.key === '?') {
          event.preventDefault();
          setShortcutsOpen((open) => !open);
          return;
        }
      }
      if ((event.ctrlKey || event.metaKey) && event.code === 'KeyZ') {
        event.preventDefault();
        if (event.shiftKey) {
          const next = annotationRedo.current.pop();
          if (next) {
            annotationUndo.current.push(annotationsRef.current);
            annotationsRef.current = next;
            setAnnotations(next);
          }
        } else {
          const previous = annotationUndo.current.pop();
          if (previous) {
            annotationRedo.current.push(annotationsRef.current);
            annotationsRef.current = previous;
            setAnnotations(previous);
          }
        }
        return;
      }
      const annotationShortcut: Record<string, AnnotationTool> = { v: 'select', t: 'text', a: 'arrow', r: 'rectangle', o: 'ellipse', p: 'pencil' };
      const tool = annotationShortcut[event.key.toLowerCase()];
      if (tool) {
        setAnnotationTool(tool);
        return;
      }
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
  }, [monitorOpen, pending, removeSelected, reshapeSelection, selectedIds.size, selectedLinkId, shortcutsOpen, temporaryPanel]);

  const canvasState = useCallback((): CanvasState => ({
    chainId: chainId ?? defaultChain ?? 1,
    scope,
    roots,
    selection,
    ranges,
    filters: filterState,
    labelMode,
    showFlow,
    removed: removedNodeIds,
    hiddenTxs,
    groups,
    dimmed,
    annotations,
    positions: positionsOf(modelRef.current),
    graph: response,
  }), [annotations, chainId, defaultChain, dimmed, filterState, groups, hiddenTxs, labelMode, ranges, removedNodeIds, response, roots, scope, selection, showFlow]);

  const currentPrint = useMemo(() => fingerprint(canvasState()), [canvasState]);
  const saveState: SaveState = saving ? 'saving' : saveError ? 'error' : canvas && savedPrint === currentPrint ? 'clean' : 'dirty';

  const documentForSave = useCallback((): { document: CanvasDocument; trimmed: boolean } => {
    const state = canvasState();
    const shared = { ...state, graph: state.graph ? withoutUserLabels(state.graph) : null };
    const full = toDocument(shared, true);
    if (documentBytes(full) <= SNAPSHOT_BUDGET) return { document: full, trimmed: false };
    return { document: toDocument(shared, false), trimmed: true };
  }, [canvasState]);

  const applyDocument = useCallback((document: CanvasDocument) => {
    const picked = chains.find((candidate) => candidate.chain_id === document.chain_id);
    if (!picked) {
      setNotice(`Chain ${document.chain_id} is not served here — opening the saved graph read-only; scans and RPC stay off until the server serves that chain.`);
    }

    requestId.current += 1;
    snapped.current = true;
    if (picked && picked.chain_id !== chainId) {
      storeChain(picked.chain_id);
      setActiveChain(picked);
      setChainId(picked.chain_id);
      setCoverage(null);
      setHistogram(null);
      setCoverageLoaded(false);
    }
    setPending(null);
    setError(null);
    setLoading(false);
    setSelectedId(null);
    setSelectedIds(new Set());
    setSelectedLinkId(null);
    setScope(document.scope);
    setRoots(document.roots);
    setRanges(document.ranges);
    setActiveRange(Math.max(document.ranges.findIndex((range) => range.from === document.selection.from && range.to === document.selection.to), 0));
    setFilterState(document.filters);
    setLabelMode(document.label_mode);
    setShowFlow(document.show_flow);
    setRemovedNodeIds(new Set(document.removed));
    setHiddenTxs(new Set(document.hidden_txs));
    setGroups(document.groups);
    setDimmed(new Set(document.dimmed));
    annotationUndo.current = [];
    annotationRedo.current = [];
    annotationsRef.current = document.annotations;
    setAnnotations(document.annotations);

    const positions = restoredPositions(document);
    modelRef.current = EMPTY_MODEL;
    restored.current = positions.size > 0 ? positions : null;
    if (document.graph) {
      setResponse(document.graph);
      setFrozen(positions.size > 0);
      if (positions.size > 0) setRestoredLayoutVersion((version) => version + 1);
    } else {
      setResponse(null);
      runAfterLoad.current = true;
    }

    return fingerprint({
      chainId: document.chain_id,
      scope: document.scope,
      roots: document.roots,
      selection: document.selection,
      ranges: document.ranges,
      filters: document.filters,
      labelMode: document.label_mode,
      showFlow: document.show_flow,
      removed: new Set(document.removed),
      hiddenTxs: new Set(document.hidden_txs),
      groups: document.groups,
      dimmed: new Set(document.dimmed),
      annotations: document.annotations,
      positions,
      graph: document.graph,
    });
  }, [chainId, chains]);

  useEffect(() => {
    if (!runAfterLoad.current) return;
    runAfterLoad.current = false;
    void run();
  }, [run]);

  const refreshUserLabels = useCallback(async (addresses: string[], chain: number | null = chainId) => {
    if (!account.me || chain === null || addresses.length === 0) return;
    const fresh = new Map<string, UserLabel[]>(addresses.map((address) => [address.toLowerCase(), []]));
    for (let start = 0; start < addresses.length; start += LABEL_CHUNK) {
      const chunk = addresses.slice(start, start + LABEL_CHUNK);
      for (const label of await labelApi.list(chain, chunk)) {
        fresh.get(label.address.toLowerCase())?.push(label);
      }
    }
    setResponse((current) => (current ? withUserLabels(current, fresh) : current));
  }, [account.me, chainId]);

  const openCanvas = useCallback(async (id: string, at?: string): Promise<boolean> => {
    try {
      const opened = await canvasApi.open(id, at);
      const document = parseDocument(opened.document);
      const print = applyDocument(document);
      if (document.graph) {
        void refreshUserLabels(document.graph.nodes.map((node) => node.address), document.chain_id).catch(() => undefined);
      }
      const fromAutosave = opened.opened?.kind === 'autosave';
      setCanvas(opened.canvas);
      setOpenedVersion(opened.opened);
      setSavedPrint(fromAutosave ? null : print);
      autosavedPrint.current = print;
      setLastAutosave(fromAutosave ? opened.opened?.created_at ?? null : null);
      setAutosaveFailed(false);
      setSaveError(null);
      setPublicView(false);
      setDialog(null);
      setUrlParam('canvas', opened.canvas.id);
      if (account.me) writeLastCanvas(account.me.user.id, opened.canvas.id);
      return true;
    } catch (cause) {
      setNotice(`Could not open the canvas: ${errorText(cause)}`);
      return false;
    }
  }, [account.me, applyDocument, refreshUserLabels]);

  const save = useCallback(async (label?: string) => {
    if (publicView) return;
    if (!account.me) {
      setDialog('auth');
      return;
    }
    if (!canvas || canvas.access === 'view') {
      setSaveError(null);
      setDialog('save');
      return;
    }

    const { document, trimmed } = documentForSave();
    const print = currentPrint;
    setSaving(true);
    setSaveError(null);
    try {
      let saved: CanvasInfo;
      try {
        saved = await canvasApi.update(canvas.id, canvas.version, { document, chain_id: document.chain_id, snapshot_label: label });
      } catch (cause) {
        if (!(cause instanceof ApiError) || cause.status !== 409) throw cause;
        if (!window.confirm(`${cause.message}\n\nOverwrite it with your version?`)) throw cause;
        const latest = await canvasApi.open(canvas.id, 'snapshot');
        saved = await canvasApi.update(canvas.id, latest.canvas.version, { document, chain_id: document.chain_id, snapshot_label: label });
      }
      setCanvas(saved);
      setSavedPrint(print);
      setOpenedVersion(null);
      autosavedPrint.current = print;
      setDialog((current) => (current === 'snapshot' ? null : current));
      if (trimmed) setNotice('The graph is too big to store whole: the canvas keeps roots, range, layout and drawings, and rebuilds the graph on open.');
    } catch (cause) {
      setSaveError(errorText(cause));
      setNotice(`Not saved: ${errorText(cause)}`);
    } finally {
      setSaving(false);
    }
  }, [account.me, canvas, currentPrint, documentForSave, publicView]);

  const saveNew = useCallback(async (title: string, owner: OwnerRequest) => {
    const { document, trimmed } = documentForSave();
    const print = currentPrint;
    setSaving(true);
    setSaveError(null);
    try {
      const created = await canvasApi.create(title, document.chain_id, owner, document);
      setCanvas(created);
      setSavedPrint(print);
      setOpenedVersion(null);
      setLastAutosave(null);
      autosavedPrint.current = print;
      setDialog(null);
      setUrlParam('canvas', created.id);
      if (account.me) writeLastCanvas(account.me.user.id, created.id);
      if (trimmed) setNotice('The graph is too big to store whole: the canvas keeps roots, range, layout and drawings, and rebuilds the graph on open.');
    } catch (cause) {
      setSaveError(errorText(cause));
    } finally {
      setSaving(false);
    }
  }, [account.me, currentPrint, documentForSave]);

  const detachCanvas = useCallback(() => {
    setCanvas(null);
    setSavedPrint(null);
    setOpenedVersion(null);
    setLastAutosave(null);
    setSaveError(null);
    setPublicView(false);
    setUrlParam('canvas', null);
    if (account.me) writeLastCanvas(account.me.user.id, null);
  }, [account.me]);

  const restoreLocalSession = useCallback(() => {
    const raw = readSession();
    if (!raw) return;
    try {
      applyDocument(parseDocument(raw));
    } catch {
      return;
    }
  }, [applyDocument]);

  const restoreSession = useCallback(async (me: Me): Promise<boolean> => {
    const last = readLastCanvas(me.user.id);
    if (!last) return false;
    if (await openCanvas(last)) return true;
    writeLastCanvas(me.user.id, null);
    return false;
  }, [openCanvas]);

  const signedIn = useCallback((me: Me) => {
    account.setMe(me);
    setDialog(null);
    if (!canvas) void restoreSession(me);
    if (response) {
      void refreshUserLabels(response.nodes.map((node) => node.address)).catch(() => undefined);
    }
  }, [account, canvas, refreshUserLabels, response, restoreSession]);

  useEffect(() => {
    if (sessionRestored.current || !chainsLoaded || !account.ready) return;
    sessionRestored.current = true;
    if (urlParam('share') || urlParam('canvas')) return;
    if (account.me) {
      void restoreSession(account.me).then((opened) => {
        if (!opened) restoreLocalSession();
      });
      return;
    }
    restoreLocalSession();
    if (account.config?.enabled) setDialog('auth');
  }, [account.config?.enabled, account.me, account.ready, chainsLoaded, restoreLocalSession, restoreSession]);

  useEffect(() => {
    if (canvas || publicView || (roots.length === 0 && !response)) return;
    const timer = window.setTimeout(() => writeSession(toDocument(canvasState(), false)), SESSION_WRITE_MS);
    return () => window.clearTimeout(timer);
  }, [canvas, canvasState, publicView, response, roots.length]);

  const autosave = useCallback(async () => {
    if (!canvas || canvas.access === 'view' || publicView || saving) return;
    const print = currentPrint;
    if (print === savedPrint || print === autosavedPrint.current) return;
    try {
      const version = await canvasApi.autosave(canvas.id, documentForSave().document);
      autosavedPrint.current = print;
      setLastAutosave(version.created_at);
      setAutosaveFailed(false);
    } catch {
      setAutosaveFailed(true);
    }
  }, [canvas, currentPrint, documentForSave, publicView, savedPrint, saving]);
  const autosaveRef = useRef(autosave);
  autosaveRef.current = autosave;

  useEffect(() => {
    const timer = window.setInterval(() => void autosaveRef.current(), AUTOSAVE_MS);
    const onHide = () => {
      if (document.visibilityState === 'hidden') void autosaveRef.current();
    };
    document.addEventListener('visibilitychange', onHide);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener('visibilitychange', onHide);
    };
  }, []);

  const signedOut = useCallback(() => {
    account.setMe(null);
    setDialog(null);
    detachCanvas();
    setResponse((current) => (current ? withoutUserLabels(current) : current));
  }, [account, detachCanvas]);

  const accountAction = useCallback((action: AccountAction) => {
    if (action === 'logout') {
      void account.logout().finally(signedOut);
      return;
    }
    setDialog(action);
  }, [account, signedOut]);

  useEffect(() => {
    if (openedFromUrl.current || !chainsLoaded || !account.ready) return;
    const token = urlParam('share');
    const id = urlParam('canvas');
    if (!token && !id) return;
    openedFromUrl.current = true;
    if (token) {
      void canvasApi.openPublic(token)
        .then((opened) => {
          applyDocument(parseDocument(opened.document));
          setCanvas(null);
          setPublicView(true);
          setNotice(`Read-only copy of “${opened.title}”.`);
        })
        .catch((cause) => setNotice(`This shared link does not open: ${errorText(cause)}`));
    } else if (id && account.me) {
      void openCanvas(id);
    } else if (id) {
      setDialog('auth');
      openedFromUrl.current = false;
    }
  }, [account.me, account.ready, applyDocument, chainsLoaded, openCanvas]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
        event.preventDefault();
        void save();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [save]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 6000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const activeScan = loading
    ? monitor.scans.find((job) => job.running && job.chain_id === chainId && job.started_ms >= loadStartedAt.current - 2000) ?? null
    : null;

  const backgroundScan = !loading
    ? monitor.scans.find((job) => job.running && job.chain_id === chainId) ?? null
    : null;

  const backgroundScanId = backgroundScan?.id ?? null;
  useEffect(() => {
    if (backgroundScanId !== null) setScanHidden(false);
  }, [backgroundScanId]);

  const activeBuild = loading && !activeScan
    ? monitor.builds.find((job) => job.running && job.chain_id === chainId && job.started_ms >= loadStartedAt.current - 2000) ?? null
    : null;

  const changeLayoutGently = useCallback(() => setIncrementalLayoutVersion((version) => version + 1), []);

  const hideNodes = useCallback((ids: string[]) => {
    changeLayoutGently();
    setRemovedNodeIds((current) => new Set([...current, ...ids]));
    setSelectedId(null);
    setSelectedIds(new Set());
  }, [changeLayoutGently]);

  const hideTxs = useCallback((hashes: string[]) => {
    changeLayoutGently();
    setHiddenTxs((current) => new Set([...current, ...hashes.map((hash) => hash.toLowerCase())]));
    setSelectedLinkId(null);
  }, [changeLayoutGently]);

  const changeGroups = useCallback((next: GraphGroup[]) => {
    setGroups((current) => {
      const visibilityChanged = current.length !== next.length
        || current.some((group, index) => group.hidden !== next[index]?.hidden || group.id !== next[index]?.id);
      if (visibilityChanged) changeLayoutGently();
      return next;
    });
  }, [changeLayoutGently]);

  const groupItems = useCallback((target: string | { name: string }, nodes: string[], txs: string[]) => {
    setGroups((current) => addToGroup(current, target, nodes, txs.map((hash) => hash.toLowerCase())));
    setNotice(typeof target === 'string' ? 'Added to the group.' : `Group “${target.name.trim() || 'new'}” created.`);
  }, []);

  const exportJson = useCallback(() => {
    const data = graphExport({
      model,
      chain,
      roots,
      selection,
      groups,
      hiddenNodes: removedNodeIds,
      hiddenTxs,
      allEdges: response?.edges ?? [],
    });
    const name = (canvas?.title ?? `ledgerscope-${chain?.slug ?? 'graph'}-${selection.from}-${selection.to}`)
      .replace(/[^\w.-]+/g, '-');
    downloadJson(data, `${name}.json`);
  }, [canvas?.title, chain, groups, hiddenTxs, model, removedNodeIds, response, roots, selection]);

  const selectedNode = selectedId ? (model.byId.get(selectedId) ?? null) : null;
  const selectedLink = selectedLinkId ? (model.links.find((link) => link.id === selectedLinkId) ?? null) : null;
  const hasGraph = model.nodes.length > 0;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <TopBar
        chains={chains}
        chain={chain}
        defaultChain={defaultChain}
        onChainChange={changeChain}
        onChainsRefresh={() => void refreshChains()}
        roots={roots}
        maxRoots={limits.maxRoots}
        scope={scope}
        onScopeChange={setScope}
        selection={selection}
        onSubmit={submitFromTopBar}
        loading={loading || buildingFromId !== null}
        advancedOpen={temporaryPanel === 'advanced'}
        onAdvancedOpenChange={(open) => setTemporaryPanel(open ? 'advanced' : null)}
        theme={theme}
        onThemeChange={setTheme}
        canvasSlot={<>
          {chains.length > 0 && (
            <Button
              variant="ghost"
              className="h-8 gap-1.5 px-2.5 text-xs"
              aria-pressed={monitorOpen}
              title={HEALTH_TITLE[monitor.health]}
              onClick={() => setMonitorOpen(!monitorOpen)}
            >
              <span className={`size-1.5 rounded-full ${HEALTH_DOT[monitor.health]}`} aria-hidden="true" />
              RPC
              {monitor.scans.some((job) => job.running) && (
                <span
                  className="cursor-pointer text-[10px] text-accent"
                  title="Show the fetch progress"
                  onClick={(event) => {
                    event.stopPropagation();
                    setScanHidden(false);
                  }}
                >
                  scanning
                </span>
              )}
            </Button>
          )}
          {account.config?.enabled && (account.me || publicView) ? (
          <CanvasControls
            canvas={canvas}
            saveState={saveState}
            publicView={publicView}
            openedVersion={openedVersion}
            lastAutosave={lastAutosave}
            autosaveFailed={autosaveFailed}
            onSave={() => void save()}
            onSnapshot={() => setDialog('snapshot')}
            onShare={() => {
              setShareTarget(canvas);
              setDialog('share');
            }}
            onBrowse={() => setDialog('canvases')}
          />
          ) : null}
        </>}
        accountSlot={account.config?.enabled ? (
          <AccountMenu me={account.me} personalRpc={account.config?.personal_rpc ?? false} onSignIn={() => setDialog('auth')} onAction={accountAction} />
        ) : null}
      />

      <main
        className="relative min-h-0 flex-1 overflow-hidden bg-plane [--inspector-w:0px] [--investigation-w:48px] [--sheet-h:40px]"
        style={
          {
            '--sheet-h': `${hasGraph && sheetOpen ? sheetHeight : SHEET_HEIGHT.collapsed}px`,
            '--investigation-w': '48px',
            '--inspector-w': selectedNode || selectedIds.size > 1 || selectedLink ? '320px' : '0px',
            '--left-flyout-top': '12px',
            '--filter-indicator-left':
              temporaryPanel === 'groups' || temporaryPanel === 'advanced' ? '372px' : temporaryPanel !== null ? '332px' : '60px',
          } as CSSProperties
        }
      >
        <GraphCanvas
          model={model}
          palette={palette}
          selectedId={selectedId}
          selectedIds={selectedIds}
          selectedLinkId={selectedLinkId}
          nodeLabels={nodeLabels}
          annotationTool={annotationTool}
          annotations={annotations}
          onAnnotationsChange={changeAnnotations}
          annotationStyle={annotationStyle}
          searchMatches={searchMatches}
          deanon={tlsLinks.score}
          ipLinks={tlsLinks.byAddress}
          labelMode={labelMode}
          showGrid
          showFlow={showFlow}
          frozen={frozen}
          incrementalLayoutVersion={incrementalLayoutVersion}
          restoredLayoutVersion={restoredLayoutVersion}
          groups={groups}
          fade={fade}
          onSelect={select}
          onSelectLink={selectLink}
          onSelectMany={selectMany}
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
          activePanel={temporaryPanel}
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
          labelMode={labelMode}
          onLabelModeChange={setLabelMode}
          groupsBadge={groups.length + (removedNodeIds.size > 0 || hiddenTxs.size > 0 ? 1 : 0)}
          advancedPanel={(
            <BlockRangePanel
              ranges={ranges}
              activeIndex={activeRange}
              coverage={coverage}
              histogram={histogram}
              limits={blockLimits}
              onActiveChange={setActiveRange}
              onRangesChange={setRanges}
              onApply={() => void run()}
              onLatest={async () => {
                try {
                  return (await fetchHead(chainId ?? undefined)).head;
                } catch (cause) {
                  setNotice(`No node told the newest block: ${errorText(cause)}`);
                  return null;
                }
              }}
              applyDisabled={loading}
            />
          )}
          ipLinks={tlsLinks.byAddress}
          groupsPanel={(
            <GroupsPanel
              groups={groups}
              edges={response?.edges ?? []}
              selection={[...selectedIds]}
              hiddenNodes={removedNodeIds}
              hiddenTxs={hiddenTxs}
              onGroupsChange={changeGroups}
              onCreateFromSelection={(name) => groupItems({ name }, [...selectedIds], [])}
              onSelectMembers={(ids) => selectMany(new Set(ids.filter((id) => model.byId.has(id))))}
              onShowNode={(id) => {
                changeLayoutGently();
                setRemovedNodeIds((current) => {
                  const next = new Set(current);
                  next.delete(id);
                  return next;
                });
              }}
              onShowAllNodes={() => {
                changeLayoutGently();
                setRemovedNodeIds(new Set());
              }}
              onShowAllTxs={() => {
                changeLayoutGently();
                setHiddenTxs(new Set());
              }}
            />
          )}
        />


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
          onExportJson={exportJson}
          focusControl={(
            <FocusControl
              settings={focus}
              onChange={changeFocus}
              bright={focusResult?.nodes.size ?? null}
              total={model.nodes.length}
              disabled={!hasGraph}
              dimmedCount={dimmed.size}
              selectedCount={selectedIds.size}
              onDimSelected={() => dimNodes([...selectedIds], true)}
              onUndimSelected={() => dimNodes([...selectedIds], false)}
              onClearDimmed={() => setDimmed(new Set())}
            />
          )}
          annotationTool={annotationTool}
          onAnnotationToolChange={setAnnotationTool}
          annotationStyle={annotationStyle}
          onAnnotationStyleChange={setAnnotationStyle}
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
              loading || roots.some((root) => root.address === selectedNode.id)
            }
            buildFromHereHint={
              roots.some((root) => root.address === selectedNode.id)
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
            actionsSlot={(
              <div className="mt-2 grid gap-1.5 border-t border-hairline pt-2">
                <GroupPicker groups={groups} onPick={(target) => groupItems(target, [selectedNode.id], [])} />
                <Button variant="ghost" className="px-2" title="Ctrl+Click on the canvas does the same" onClick={() => select(selectedNode.id, 'neighborhood')}>
                  Select with its counterparties
                </Button>
                <div className="grid grid-cols-2 gap-1.5">
                  <Button variant="ghost" className="px-2" onClick={() => dimNodes([selectedNode.id], !dimmed.has(selectedNode.id))}>
                    <IconFocus size={14} /> {dimmed.has(selectedNode.id) ? 'Restore' : 'Fade'}
                  </Button>
                  <Button variant="ghost" className="px-2" onClick={() => hideNodes([selectedNode.id])}>
                    <IconEyeOff size={14} /> Hide
                  </Button>
                </div>
              </div>
            )}
            onPickTag={(tag) => setFilterState((current) => ({ ...current, tag }))}
            labelsSlot={account.config?.enabled ? (
              <NodeLabels
                me={account.me}
                chainId={chainId}
                address={selectedNode.id}
                readOnly={publicView}
                onSignIn={() => setDialog('auth')}
                onChanged={(address, labels) => setResponse((current) => (
                  current ? withUserLabels(current, new Map([[address.toLowerCase(), labels]])) : current
                ))}
              />
            ) : null}
          />
        )}

        {selectedIds.size > 1 && (
          <MultiSelectionInspector
            ids={selectedIds}
            model={model}
            onRemove={removeSelected}
            onClear={() => select(null)}
            groups={groups}
            onGroup={(target) => groupItems(target, [...selectedIds], [])}
            onFade={() => dimNodes([...selectedIds], true)}
          />
        )}

        {selectedLink && (
          <EdgeInspector
            link={selectedLink}
            groups={groups}
            onGroup={(target) => groupItems(target, [], selectedLink.txs.map((tx) => tx.tx_hash))}
            onHide={() => hideTxs(selectedLink.txs.map((tx) => tx.tx_hash))}
            onPickTag={(tag) => setFilterState((current) => ({ ...current, tag }))}
            onClose={() => selectLink(null)}
            onShowTransactions={() => {
              setSheetScope(null);
              setSheetLinkScope(selectedLink.id);
              setSheetOpen(true);
            }}
          />
        )}

        {visibleResponse && visibleResponse.edges.length > 0 && (
          <Timeline
            edges={visibleResponse.edges}
            window={filterState.window}
            onWindowChange={(window) => setFilterState((current) => ({ ...current, window }))}
            open={timelineOpen}
            onOpenChange={setTimelineOpen}
          />
        )}

        {hasGraph && (
          <TxSheet
            model={model}
            scope={sheetScope}
            linkScope={sheetLinkScope}
            open={sheetOpen}
            height={sheetHeight}
            onOpenChange={(open) => {
              if (open) setTemporaryPanel(null);
              setSheetOpen(open);
            }}
            onHeightChange={setSheetHeight}
            onScopeClear={() => { setSheetScope(null); setSheetLinkScope(null); }}
            groups={groups}
            onHideTxs={hideTxs}
            onGroupTxs={(target, hashes) => groupItems(target, [], hashes)}
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
            <Panel className="pointer-events-auto grid w-[min(440px,calc(100vw-48px))] grid-cols-[minmax(0,1fr)] justify-items-center gap-2 px-6 py-[22px]">
              <div className="size-[22px] animate-spin-exact rounded-full border-2 border-hairline-strong border-t-accent" aria-hidden="true" />
              <h2 className="m-0 text-base font-semibold">{activeScan ? (activeScan.source === 'dataset' ? 'Fetching blocks from the AWS dataset' : 'Fetching blocks over RPC') : 'Building investigation graph'}</h2>
              {activeScan ? (
                <ScanProgress job={activeScan} calls={monitor.calls} onOpenMonitor={() => setMonitorOpen(true)} />
              ) : activeBuild ? (
                <BuildProgress job={activeBuild} sweep={scope === 'blocks'} />
              ) : (
                <p className="m-0 text-[13px] text-text-secondary">
                  Stored blocks answer immediately. Missing live blocks can take longer over JSON-RPC.
                </p>
              )}
              <Button className="mt-1" onClick={stopFetching}>
                {activeScan ? 'Stop fetching' : 'Cancel'}
              </Button>
            </Panel>
          </div>
        )}

        {!loading && !pending && error && (
          <div className="pointer-events-none absolute bottom-[var(--sheet-h)] left-[var(--investigation-w)] right-[var(--inspector-w)] top-0 z-11 grid place-content-center p-6 text-center">
            <Panel className="pointer-events-auto grid max-w-[440px] justify-items-center gap-2 px-6 py-[22px]">
              <h2 className="m-0 text-base font-semibold text-critical">Could not load the graph</h2>
              <p className="m-0 text-[13px] text-text-secondary">{error}</p>
              <Button variant="primary" onClick={() => void run()}>Retry</Button>
            </Panel>
          </div>
        )}

        {!loading && !error && !pending && !hasGraph && (
          <div className="pointer-events-none absolute bottom-[var(--sheet-h)] left-[var(--investigation-w)] right-0 top-0 z-11 grid place-content-center p-6 text-center">
            <div className="grid max-w-[420px] justify-items-center gap-2 px-6 py-[22px]">
              <h2 className="m-0 text-base font-semibold">Start an investigation</h2>
              <p className="m-0 text-[13px] text-text-secondary">Paste an Ethereum address above to add it to the canvas.</p>
              <p className="m-0 text-xs text-text-muted">Explore transfers, counterparties and transaction paths.</p>
              <div className="mt-2 text-[11px] text-text-muted"><Kbd>/</Kbd> filters · <Kbd>f</Kbd> fit · <Kbd>space</Kbd> freeze · <Kbd>?</Kbd> selection keys</div>
            </div>
          </div>
        )}
        {shortcutsOpen && <ShortcutsHelp onClose={() => setShortcutsOpen(false)} />}

        {hasGraph && (canvasSearchOpen ? (
          <CanvasSearch
            query={search}
            hits={searchHits}
            onQueryChange={setSearch}
            onPick={(id) => {
              select(id);
              graph.current?.centerOn(id);
            }}
            onClose={() => setCanvasSearchOpen(false)}
          />
        ) : (
          <Panel className="absolute right-[calc(var(--inspector-w)+72px)] top-4 z-10 p-[5px]">
            <Button variant="ghost" className="h-8 gap-1.5 px-2.5 text-xs" title="Find wallets on the canvas (Ctrl+F)" onClick={() => setCanvasSearchOpen(true)}>
              <IconSearch size={14} />
              Find wallet
              {search.trim().length >= MIN_QUERY && <span className="tabular-nums text-accent">{formatCount(searchMatches.size)}</span>}
            </Button>
          </Panel>
        ))}

        {backgroundScan && !scanHidden && (
          <Panel
            className="absolute left-1/2 top-3 z-20 grid w-[min(420px,calc(100vw-120px))] -translate-x-1/2 gap-2 px-4 py-3 shadow-pop"
            role="status"
            aria-label="Blocks being fetched"
          >
            <div className="flex items-center justify-between gap-2">
              <h2 className="m-0 text-[13px] font-semibold">Fetching blocks in the background</h2>
              <Button variant="ghost" className="h-7 px-2 text-xs" onClick={() => setScanHidden(true)}>Hide</Button>
            </div>
            <ScanProgress job={backgroundScan} calls={monitor.calls} onOpenMonitor={() => setMonitorOpen(true)} />
          </Panel>
        )}

        {monitorOpen && (
          <MonitorPanel monitor={monitor} chains={chains} chainId={chainId} onClose={() => setMonitorOpen(false)} />
        )}

        {notice && (
          <Panel className="absolute bottom-[calc(var(--sheet-h)+108px)] left-1/2 z-40 flex max-w-[560px] -translate-x-1/2 items-center gap-3 px-4 py-2.5 text-xs text-text-secondary shadow-pop" role="status">
            <span>{notice}</span>
            <Button variant="ghost" className="h-7 px-2" onClick={() => setNotice(null)}>Dismiss</Button>
          </Panel>
        )}
      </main>

      {dialog === 'auth' && account.config && (
        <AuthDialog registrationOpen={account.config.registration_open} onSignedIn={signedIn} onClose={() => setDialog(null)} />
      )}
      {dialog === 'profile' && account.me && (
        <ProfileDialog me={account.me} onChanged={account.setMe} onSignedOutEverywhere={signedOut} onClose={() => setDialog(null)} />
      )}
      {dialog === 'teams' && account.me && (
        <TeamsDialog me={account.me} onTeamsChanged={account.refresh} onClose={() => setDialog(null)} />
      )}
      {dialog === 'canvases' && account.me && (
        <CanvasesDialog
          me={account.me}
          chains={chains}
          currentId={canvas?.id ?? null}
          onOpen={(picked, at) => {
            if (canvas && saveState === 'dirty' && autosavedPrint.current !== currentPrint && !window.confirm('Your canvas has changes that are not saved or autosaved yet. Open another one anyway?')) return;
            void openCanvas(picked.id, at);
          }}
          onChanged={(changed) => {
            if (canvas?.id === changed.id) setCanvas(changed);
          }}
          onShare={(picked) => {
            setShareTarget(picked);
            setDialog('share');
          }}
          onDeleted={(id) => {
            if (canvas?.id === id) detachCanvas();
          }}
          onSaveNew={() => {
            setSaveError(null);
            setDialog('save');
          }}
          onDetach={canvas ? () => {
            detachCanvas();
            setDialog(null);
          } : undefined}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === 'save' && account.me && (
        <SaveCanvasDialog
          me={account.me}
          initialTitle={canvas ? `${canvas.title} (copy)` : roots[0] ? `Investigation ${roots[0].address.slice(0, 10)}` : 'Investigation'}
          busy={saving}
          error={saveError}
          onSave={(title, owner) => void saveNew(title, owner)}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === 'snapshot' && canvas && (
        <SnapshotDialog
          canvasTitle={canvas.title}
          busy={saving}
          error={saveError}
          onSave={(label) => void save(label)}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === 'share' && shareTarget && (
        <ShareDialog canvas={shareTarget} onClose={() => setDialog(null)} />
      )}
      {dialog === 'labels' && account.me && (
        <LabelsDialog
          chains={chains}
          chainId={chainId}
          onFocusAddress={(address) => {
            const id = address.toLowerCase();
            if (model.byId.has(id)) {
              setDialog(null);
              setSelectedId(id);
              graph.current?.centerOn(id);
            } else {
              setNotice('This address is not on the current graph.');
            }
          }}
          onChanged={(address) => void refreshUserLabels([address]).catch(() => undefined)}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog === 'info' && account.me && (
        <InfoDialog onClose={() => setDialog(null)} />
      )}
      {dialog === 'rpc' && account.me && (
        <RpcNodesDialog chains={chains} chainId={chainId} onClose={() => setDialog(null)} />
      )}
      {dialog === 'tls' && account.me && (
        <TlsDeanonDialog
          chains={chains}
          chainId={chainId}
          canvasRange={hasGraph ? { from: Math.min(...ranges.map((range) => range.from)), to: Math.max(...ranges.map((range) => range.to)) } : undefined}
          onChange={() => void tlsLinks.refresh()}
          onClose={() => setDialog(null)}
        />
      )}
    </div>
  );
}
