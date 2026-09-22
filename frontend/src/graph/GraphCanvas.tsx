import { useEffect, useImperativeHandle, useRef, useState, type RefObject } from 'react';
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
} from 'd3-force';

import { formatCount, formatEth, formatUnits, shortAddress } from '../lib/format';
import type { VizPalette } from '../lib/theme';
import { Dot } from '../components/ui';
import {
  contentBounds,
  drawScene,
  findLinkAt,
  findNodeAt,
  toWorld,
  type LabelMode,
  type Transform,
} from './draw';
import { assetLabel, netFlow, type GraphLink, type GraphModel, type GraphNode } from './model';
import type { Annotation, AnnotationKind, AnnotationStyle, AnnotationTool } from './annotations';
import { getPrimarySystemLabel } from './labels';

export interface GraphHandle {
  fit(): void;
  zoomBy(factor: number): void;
  centerOn(id: string): void;
  reheat(): void;
  unpinAll(): void;
  exportPng(filename: string): void;
}

type Hover = { type: 'node'; node: GraphNode } | { type: 'link'; link: GraphLink };

interface Props {
  model: GraphModel;
  palette: VizPalette;
  selectedId: string | null;
  selectedIds: Set<string>;
  selectedLinkId: string | null;
  nodeLabels: Map<string, string>;
  annotationTool: AnnotationTool;
  annotations: Annotation[];
  onAnnotationsChange: (next: Annotation[]) => void;
  annotationStyle: AnnotationStyle;
  searchMatches: Set<string>;
  labelMode: LabelMode;
  showGrid: boolean;
  showFlow: boolean;
  frozen: boolean;
  incrementalLayoutVersion: number;
  onSelect: (id: string | null, additive?: boolean) => void;
  onSelectLink: (id: string | null) => void;
  onSelectMany: (ids: Set<string>) => void;
  handle: RefObject<GraphHandle | null>;
}

const MIN_ZOOM = 0.04;
const MAX_ZOOM = 6;
const CLICK_SLOP = 4;

function drawAnnotations(ctx: CanvasRenderingContext2D, annotations: Annotation[], transform: Transform, dpr: number, color: string) {
  ctx.save();
  ctx.setTransform(dpr * transform.k, 0, 0, dpr * transform.k, dpr * transform.x, dpr * transform.y);
  ctx.globalAlpha = 0.86;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  for (const item of annotations) {
    ctx.strokeStyle = item.color ?? color;
    ctx.fillStyle = item.color ?? color;
    ctx.lineWidth = (item.strokeWidth ?? 2) / transform.k;
    ctx.font = `500 ${(item.fontSize ?? 14) / transform.k}px Inter Variable, system-ui`;
    const stroke = item.strokeWidth ?? 2;
    const dx = item.x2 - item.x;
    const dy = item.y2 - item.y;
    const arrowLength = Math.hypot(dx, dy);
    const direction = arrowLength > 0 ? { x: dx / arrowLength, y: dy / arrowLength } : null;
    const headLength = direction
      ? Math.min(Math.max(stroke * 3 / transform.k, 10 / transform.k), 26 / transform.k, arrowLength * 0.4)
      : 0;
    const headWidth = Math.min(Math.max(stroke * 2 / transform.k, 8 / transform.k), 18 / transform.k);
    const base = direction
      ? { x: item.x2 - direction.x * headLength, y: item.y2 - direction.y * headLength }
      : null;
    ctx.lineCap = item.kind === 'arrow' ? 'butt' : 'round';
    const x = Math.min(item.x, item.x2); const y = Math.min(item.y, item.y2);
    const w = Math.abs(item.x2 - item.x); const h = Math.abs(item.y2 - item.y);
    ctx.beginPath();
    if (item.kind === 'rectangle') ctx.roundRect(x, y, w, h, Math.min(6 / transform.k, w / 5, h / 5));
    else if (item.kind === 'ellipse') ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
    else if (item.kind === 'pencil' && item.points) {
      const points = item.points;
      if (points.length === 1) {
        ctx.arc(points[0].x, points[0].y, Math.max((item.strokeWidth ?? 2) / transform.k / 2, 1 / transform.k), 0, Math.PI * 2);
        ctx.fill();
        continue;
      }
      ctx.moveTo(points[0].x, points[0].y);
      for (let index = 1; index < points.length - 1; index += 1) {
        const point = points[index];
        const next = points[index + 1];
        ctx.quadraticCurveTo(point.x, point.y, (point.x + next.x) / 2, (point.y + next.y) / 2);
      }
      const last = points[points.length - 1];
      ctx.lineTo(last.x, last.y);
    }
    else if (item.kind === 'text') { ctx.fillText(item.text ?? '', item.x, item.y); continue; }
    else if (item.kind === 'arrow') {
      ctx.moveTo(item.x, item.y);
      if (base) ctx.lineTo(base.x, base.y);
    } else { ctx.moveTo(item.x, item.y); ctx.lineTo(item.x2, item.y2); }
    ctx.stroke();
    if (item.kind === 'arrow') {
      if (!direction || !base) continue;
      // Butt cap prevents the shaft protruding through the filled head; draw a
      // round tail separately to retain the friendly line ending at the start.
      ctx.beginPath();
      ctx.arc(item.x, item.y, ctx.lineWidth / 2, 0, Math.PI * 2);
      ctx.fill();
      const perpendicular = { x: -direction.y, y: direction.x };
      ctx.beginPath();
      ctx.moveTo(item.x2, item.y2);
      ctx.lineTo(base.x + perpendicular.x * headWidth / 2, base.y + perpendicular.y * headWidth / 2);
      ctx.lineTo(base.x - perpendicular.x * headWidth / 2, base.y - perpendicular.y * headWidth / 2);
      ctx.closePath();
      ctx.fill();
    }
  }
  ctx.restore();
}

function annotationGeometryBounds(item: Annotation, transform: Transform) {
  const points = item.points ?? [];
  const xs = [item.x, item.x2, ...points.map((point) => point.x)];
  const ys = [item.y, item.y2, ...points.map((point) => point.y)];
  let minX = Math.min(...xs); let maxX = Math.max(...xs);
  const minY = Math.min(...ys); const maxY = Math.max(...ys);
  if (item.kind === 'text') maxX = item.x + Math.max(80, (item.text?.length ?? 1) * (item.fontSize ?? 14) * .58) / transform.k;
  return { minX, minY, maxX, maxY };
}

function annotationBounds(item: Annotation, transform: Transform) {
  const geometry = annotationGeometryBounds(item, transform);
  const pad = Math.max(8, item.fontSize ?? 14) / transform.k;
  const minX = geometry.minX - pad; const maxX = geometry.maxX + pad;
  const minY = geometry.minY - pad; const maxY = geometry.maxY + pad;
  return { minX, minY, maxX, maxY };
}

function annotationsGeometryBounds(items: Annotation[], transform: Transform) {
  const boxes = items.map((item) => annotationGeometryBounds(item, transform));
  return { minX: Math.min(...boxes.map((box) => box.minX)), minY: Math.min(...boxes.map((box) => box.minY)), maxX: Math.max(...boxes.map((box) => box.maxX)), maxY: Math.max(...boxes.map((box) => box.maxY)) };
}

function drawAnnotationSelection(ctx: CanvasRenderingContext2D, item: Annotation, transform: Transform, dpr: number, color: string) {
  const box = annotationBounds(item, transform);
  const x = box.minX * transform.k + transform.x;
  const y = box.minY * transform.k + transform.y;
  const w = (box.maxX - box.minX) * transform.k;
  const h = (box.maxY - box.minY) * transform.k;
  ctx.save(); ctx.setTransform(dpr, 0, 0, dpr, 0, 0); ctx.strokeStyle = color; ctx.lineWidth = 1;
  ctx.setLineDash([4, 3]); ctx.strokeRect(x, y, w, h); ctx.setLineDash([]);
  ctx.fillStyle = color; ctx.fillRect(x + w - 4, y + h - 4, 8, 8); ctx.restore();
}

export function GraphCanvas({
  model,
  palette,
  selectedId,
  selectedIds,
  selectedLinkId,
  nodeLabels,
  annotationTool,
  annotations,
  onAnnotationsChange,
  annotationStyle,
  searchMatches,
  labelMode,
  showGrid,
  showFlow,
  frozen,
  incrementalLayoutVersion,
  onSelect,
  onSelectLink,
  onSelectMany,
  handle,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);

  const transform = useRef<Transform>({ x: 0, y: 0, k: 1 });
  const size = useRef({ width: 0, height: 0, dpr: 1 });
  const simulation = useRef<Simulation<GraphNode, GraphLink> | null>(null);
  const renderedModel = useRef<GraphModel | null>(null);
  const handledLayoutVersion = useRef(incrementalLayoutVersion);

  const hoveredNode = useRef<GraphNode | null>(null);
  const hoveredLink = useRef<GraphLink | null>(null);
  const focusMix = useRef(0);
  const retained = useRef<{ nodes: Set<string>; links: Set<string> } | null>(null);
  const marquee = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const draftAnnotation = useRef<Annotation | null>(null);
  const groupPreview = useRef<Annotation[] | null>(null);
  const [textEditor, setTextEditor] = useState<{ x: number; y: number } | null>(null);
  const [selectedAnnotationIds, setSelectedAnnotationIds] = useState<Set<string>>(() => new Set());
  const textInputRef = useRef<HTMLInputElement>(null);
  const dirty = useRef(true);

  const fitMode = useRef<'off' | 'once'>('once');

  const [hover, setHover] = useState<Hover | null>(null);

  const live = useRef({ model, palette, selectedId, selectedIds, selectedLinkId, nodeLabels, annotationTool, annotations, annotationStyle, selectedAnnotationIds, searchMatches, labelMode, showGrid, showFlow });
  live.current = { model, palette, selectedId, selectedIds, selectedLinkId, nodeLabels, annotationTool, annotations, annotationStyle, selectedAnnotationIds, searchMatches, labelMode, showGrid, showFlow };
  useEffect(() => {
    dirty.current = true;
  }, [palette, selectedId, selectedIds, selectedLinkId, nodeLabels, annotationTool, annotations, annotationStyle, selectedAnnotationIds, searchMatches, labelMode, showGrid, showFlow]);

  useEffect(() => {
    const nodes = model.nodes;
    const previous = renderedModel.current;
    const preserveExisting =
      incrementalLayoutVersion !== handledLayoutVersion.current && previous !== null;
    handledLayoutVersion.current = incrementalLayoutVersion;
    renderedModel.current = model;
    hoveredNode.current = null;
    hoveredLink.current = null;
    setHover(null);

    if (nodes.length === 0) {
      simulation.current?.stop();
      simulation.current = null;
      dirty.current = true;
      return;
    }

    const restoredPins = new Map<string, { fx?: number | null; fy?: number | null }>();
    if (preserveExisting && previous) {
      for (const oldNode of previous.nodes) {
        const node = model.byId.get(oldNode.id);
        if (!node) continue;
        restoredPins.set(node.id, { fx: node.fx, fy: node.fy });
        node.fx = node.x;
        node.fy = node.y;
      }
    }

    const sim = forceSimulation<GraphNode, GraphLink>(nodes)
      .force(
        'link',
        forceLink<GraphNode, GraphLink>(model.links).distance(
          (link) => 46 + link.source.r + link.target.r + 26 / (1 + link.count),
        ),
      )
      .force(
        'charge',
        forceManyBody<GraphNode>()
          .strength((node) => -110 - node.r * 14)
          .distanceMax(1200),
      )
      .force('collide', forceCollide<GraphNode>((node) => node.r + 7).iterations(2))
      .force('x', forceX(0).strength(0.035))
      .force('y', forceY(0).strength(0.035))
      .velocityDecay(0.42)
      .alphaDecay(0.045)
      .alphaMin(0.01);

    sim.stop();
    const budget = preserveExisting
      ? nodes.length > 1200
        ? 45
        : nodes.length > 400
          ? 75
          : 110
      : nodes.length > 1200
        ? 90
        : nodes.length > 400
          ? 170
          : 320;
    for (let i = 0; i < budget && sim.alpha() > sim.alphaMin(); i += 1) sim.tick();

    for (const [id, pin] of restoredPins) {
      const node = model.byId.get(id);
      if (!node) continue;
      node.fx = pin.fx;
      node.fy = pin.fy;
    }

    simulation.current = sim;
    sim.on('tick', () => {
      dirty.current = true;
    });

    fitMode.current = preserveExisting ? 'off' : 'once';
    dirty.current = true;

    return () => {
      sim.on('tick', null);
      sim.stop();
    };
  }, [incrementalLayoutVersion, model]);

  useEffect(() => {
    const sim = simulation.current;
    if (!sim) return;
    if (frozen) sim.stop();
    dirty.current = true;
  }, [frozen, model]);

  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;
    const snapshot = document.createElement('canvas');

    const apply = () => {
      const rect = container.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.style.width = '100%';
      canvas.style.height = '100%';
      const pixelWidth = Math.max(1, Math.round(rect.width * dpr));
      const pixelHeight = Math.max(1, Math.round(rect.height * dpr));
      size.current = { width: rect.width, height: rect.height, dpr };
      if (canvas.width === pixelWidth && canvas.height === pixelHeight) return;

      // Resizing a canvas clears it immediately. Preserve the current pixels
      // at their exact size so a ResizeObserver paint between this callback
      // and the render loop never flashes blank or rubber-stretches the graph.
      snapshot.width = canvas.width;
      snapshot.height = canvas.height;
      snapshot.getContext('2d')?.drawImage(canvas, 0, 0);
      canvas.width = pixelWidth;
      canvas.height = pixelHeight;
      const resized = canvas.getContext('2d');
      if (resized) {
        resized.fillStyle = live.current.palette.plane;
        resized.fillRect(0, 0, pixelWidth, pixelHeight);
        resized.drawImage(snapshot, 0, 0);
      }
      dirty.current = true;
    };

    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (selectedAnnotationIds.size === 0) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target?.isContentEditable) return;
      if (event.key === 'Escape') setSelectedAnnotationIds(new Set());
      if (event.key === 'Delete' || event.key === 'Backspace') {
        event.preventDefault();
        onAnnotationsChange(live.current.annotations.filter((item) => !selectedAnnotationIds.has(item.id)));
        setSelectedAnnotationIds(new Set());
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onAnnotationsChange, selectedAnnotationIds]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;

    let frame = requestAnimationFrame(function render(time: number) {
      frame = requestAnimationFrame(render);
      const { width, height, dpr } = size.current;
      if (width === 0 || height === 0) return;

      const state = live.current;
      const active = hoveredNode.current?.id ?? state.selectedId;
      const hoverLink = hoveredLink.current;
      const settling = (simulation.current?.alpha() ?? 0) > 0.005 && !frozen;

      if (fitMode.current === 'once' && state.model.nodes.length > 0) {
        const settled = ease(transform.current, fitTarget(state.model, size.current));
        dirty.current = true;
        if (settled) fitMode.current = 'off';
      }

      let sets = retained.current;
      if (active || hoverLink) {
        const nodeIds = new Set<string>();
        const linkIds = new Set<string>();
        if (active) {
          nodeIds.add(active);
          for (const neighbor of state.model.neighbors.get(active) ?? []) nodeIds.add(neighbor);
          for (const link of state.model.linksByNode.get(active) ?? []) linkIds.add(link.id);
        } else if (hoverLink) {
          linkIds.add(hoverLink.id);
          nodeIds.add(hoverLink.source.id);
          nodeIds.add(hoverLink.target.id);
        }
        sets = { nodes: nodeIds, links: linkIds };
        retained.current = sets;
      }

      const target = active || hoverLink ? 1 : 0;
      const before = focusMix.current;
      focusMix.current += (target - focusMix.current) * 0.22;
      if (Math.abs(target - focusMix.current) < 0.004) focusMix.current = target;
      const fading = focusMix.current !== before;
      if (focusMix.current === 0) {
        sets = null;
        retained.current = null;
      }

      const flowing = state.showFlow && !!sets && sets.links.size > 0;
      if (!dirty.current && !settling && !fading && !flowing) return;
      dirty.current = false;

      drawScene(ctx, {
        model: state.model,
        palette: state.palette,
        transform: transform.current,
        width,
        height,
        dpr,
        activeId: active,
        selectedId: state.selectedId,
        selectedIds: state.selectedIds,
        selectedLinkId: state.selectedLinkId,
        nodeLabels: state.nodeLabels,
        highlightNodes: sets ? sets.nodes : null,
        highlightLinks: sets ? sets.links : null,
        searchMatches: state.searchMatches,
        focusMix: focusMix.current,
        labelMode: state.labelMode,
        time,
        showGrid: state.showGrid,
        showFlow: state.showFlow,
        // The canvas owns the background so drawGrid uses the exact same
        // world transform as nodes and links rather than a screen-fixed CSS pattern.
        transparentBackground: false,
      });
      const renderedAnnotations = groupPreview.current
        ? state.annotations.map((item) => groupPreview.current?.find((preview) => preview.id === item.id) ?? item)
        : state.annotations;
      drawAnnotations(ctx, renderedAnnotations, transform.current, dpr, state.palette.focus);
      if (draftAnnotation.current) drawAnnotations(ctx, [draftAnnotation.current], transform.current, dpr, state.palette.focus);
      for (const selected of renderedAnnotations.filter((item) => state.selectedAnnotationIds.has(item.id))) drawAnnotationSelection(ctx, selected, transform.current, dpr, selected.color ?? state.palette.focus);
      if (marquee.current) {
        const box = marquee.current;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.fillStyle = state.palette.focus;
        ctx.globalAlpha = 0.12;
        ctx.fillRect(box.x, box.y, box.width, box.height);
        ctx.globalAlpha = 1;
        ctx.strokeStyle = state.palette.focus;
        ctx.lineWidth = 1;
        ctx.setLineDash([4, 3]);
        ctx.strokeRect(box.x + 0.5, box.y + 0.5, box.width, box.height);
        ctx.setLineDash([]);
      }
    });

    return () => cancelAnimationFrame(frame);
  }, [frozen]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let downAt: { x: number; y: number } | null = null;
    let dragNodes: GraphNode[] | null = null;
    let dragOrigin: { x: number; y: number } | null = null;
    let dragPositions: Array<{ node: GraphNode; x: number; y: number }> = [];
    let panning = false;
    let selecting = false;
    let selectionStart: { x: number; y: number } | null = null;
    let selectionRect: { x: number; y: number; width: number; height: number } | null = null;
    let annotationEdit: { original: Annotation; mode: 'move' | 'resize'; origin: { x: number; y: number } } | null = null;
    let groupEdit: { originals: Annotation[]; origin: { x: number; y: number }; bounds: ReturnType<typeof annotationsGeometryBounds>; handle?: 'nw' | 'ne' | 'sw' | 'se' } | null = null;
    let moved = 0;

    const localPoint = (event: PointerEvent | WheelEvent) => {
      const rect = canvas.getBoundingClientRect();
      return { x: event.clientX - rect.left, y: event.clientY - rect.top };
    };

    const placeTooltip = (point: { x: number; y: number }) => {
      const tip = tooltipRef.current;
      if (!tip) return;
      const { width, height } = size.current;
      const flipX = point.x > width - tip.offsetWidth - 28;
      const flipY = point.y > height - tip.offsetHeight - 28;
      const x = flipX ? point.x - tip.offsetWidth - 16 : point.x + 16;
      const y = flipY ? point.y - tip.offsetHeight - 16 : point.y + 16;
      tip.style.transform = `translate3d(${Math.round(x)}px, ${Math.round(y)}px, 0)`;
    };

    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      fitMode.current = 'off';
      const point = localPoint(event);
      const t = transform.current;
      const factor = Math.exp(-event.deltaY * (event.deltaMode === 1 ? 0.05 : 0.0016));
      const k = clamp(t.k * factor, MIN_ZOOM, MAX_ZOOM);

      t.x = point.x - ((point.x - t.x) / t.k) * k;
      t.y = point.y - ((point.y - t.y) / t.k) * k;
      t.k = k;
      dirty.current = true;
    };

    const onPointerDown = (event: PointerEvent) => {
      if (event.button !== 0 && event.button !== 1) return;
      fitMode.current = 'off';
      canvas.setPointerCapture(event.pointerId);
      const point = localPoint(event);
      downAt = point;
      moved = 0;

      const world = toWorld(transform.current, point.x, point.y);
      if (event.button === 0 && live.current.annotationTool === 'select') {
        const selectedGroup = live.current.annotations.filter((item) => live.current.selectedAnnotationIds.has(item.id));
        if (selectedGroup.length > 1) {
          const bounds = annotationsGeometryBounds(selectedGroup, transform.current);
          const corners = { nw: [bounds.minX, bounds.minY], ne: [bounds.maxX, bounds.minY], sw: [bounds.minX, bounds.maxY], se: [bounds.maxX, bounds.maxY] } as const;
          const handle = (Object.entries(corners) as Array<['nw' | 'ne' | 'sw' | 'se', readonly [number, number]]>)
            .find(([, corner]) => Math.abs(point.x - (corner[0] * transform.current.k + transform.current.x)) <= 18 && Math.abs(point.y - (corner[1] * transform.current.k + transform.current.y)) <= 18)?.[0];
          if (handle) { groupEdit = { originals: selectedGroup, origin: world, bounds, handle }; canvas.style.cursor = 'nwse-resize'; return; }
        }
        const selected = live.current.selectedAnnotationIds.size === 1
          ? live.current.annotations.find((item) => live.current.selectedAnnotationIds.has(item.id))
          : null;
        if (selected) {
          const box = annotationBounds(selected, transform.current);
          const handleX = box.maxX * transform.current.k + transform.current.x;
          const handleY = box.maxY * transform.current.k + transform.current.y;
          // This matches the actual rendered handle, independent of zoom.
          if (Math.abs(point.x - handleX) <= 18 && Math.abs(point.y - handleY) <= 18) {
            annotationEdit = { original: selected, mode: 'resize', origin: world };
            canvas.style.cursor = 'nwse-resize';
            return;
          }
        }
        const hit = [...live.current.annotations].reverse().find((item) => {
          const box = annotationBounds(item, transform.current);
          return world.x >= box.minX && world.x <= box.maxX && world.y >= box.minY && world.y <= box.maxY;
        });
        if (hit) {
          const group = live.current.annotations.filter((item) => live.current.selectedAnnotationIds.has(item.id));
          if (group.length > 1 && live.current.selectedAnnotationIds.has(hit.id) && !event.shiftKey) {
            groupEdit = { originals: group, origin: world, bounds: annotationsGeometryBounds(group, transform.current) };
            canvas.style.cursor = 'move';
            return;
          }
          setSelectedAnnotationIds((current) => {
            if (!event.shiftKey) return new Set([hit.id]);
            const next = new Set(current);
            if (next.has(hit.id)) next.delete(hit.id); else next.add(hit.id);
            return next;
          });
          const box = annotationBounds(hit, transform.current);
          const handle = 20 / transform.current.k;
          const mode = Math.abs(world.x - box.maxX) <= handle && Math.abs(world.y - box.maxY) <= handle ? 'resize' : 'move';
          annotationEdit = { original: hit, mode, origin: world };
          canvas.style.cursor = mode === 'resize' ? 'nwse-resize' : 'move';
          return;
        }
        if (!event.shiftKey) setSelectedAnnotationIds(new Set());
      }
      if (event.button === 0 && live.current.annotationTool !== 'select') {
        const kind = live.current.annotationTool as AnnotationKind;
        draftAnnotation.current = { id: crypto.randomUUID(), kind, x: world.x, y: world.y, x2: world.x, y2: world.y, points: kind === 'pencil' ? [{ x: world.x, y: world.y }] : undefined, ...live.current.annotationStyle };
        canvas.style.cursor = 'crosshair';
        return;
      }
      const node =
        event.button === 0 ? findNodeAt(live.current.model, world, transform.current.k) : null;
      if (node) {
        const selected = live.current.selectedIds.has(node.id);
        dragNodes = selected
          ? live.current.model.nodes.filter((candidate) => live.current.selectedIds.has(candidate.id))
          : [node];
        dragOrigin = world;
        dragPositions = dragNodes.map((candidate) => ({ node: candidate, x: candidate.x, y: candidate.y }));
        for (const candidate of dragNodes) {
          candidate.fx = candidate.x;
          candidate.fy = candidate.y;
        }
        if (!frozen) simulation.current?.alphaTarget(0.28).restart();
      } else if (event.button === 0) {
        selecting = true;
        selectionStart = point;
        selectionRect = { x: point.x, y: point.y, width: 0, height: 0 };
        marquee.current = selectionRect;
      } else {
        panning = true;
      }
      canvas.style.cursor = 'grabbing';
    };

    const onPointerMove = (event: PointerEvent) => {
      const point = localPoint(event);

      if (groupEdit) {
        const world = toWorld(transform.current, point.x, point.y);
        const dx = world.x - groupEdit.origin.x; const dy = world.y - groupEdit.origin.y;
        if (!groupEdit.handle) {
          groupPreview.current = groupEdit.originals.map((item) => ({ ...item, x: item.x + dx, y: item.y + dy, x2: item.x2 + dx, y2: item.y2 + dy, points: item.points?.map((value) => ({ x: value.x + dx, y: value.y + dy })) }));
        } else {
          const { bounds, handle } = groupEdit;
          const anchorX = handle.includes('w') ? bounds.maxX : bounds.minX;
          const anchorY = handle.includes('n') ? bounds.maxY : bounds.minY;
          const baseX = handle.includes('w') ? bounds.minX : bounds.maxX;
          const baseY = handle.includes('n') ? bounds.minY : bounds.maxY;
          const scaleX = Math.max(Math.abs((world.x - anchorX) / (baseX - anchorX)), .04);
          const scaleY = Math.max(Math.abs((world.y - anchorY) / (baseY - anchorY)), .04);
          const scale = (x: number, y: number) => ({ x: anchorX + (x - anchorX) * scaleX, y: anchorY + (y - anchorY) * scaleY });
          groupPreview.current = groupEdit.originals.map((item) => { const a = scale(item.x, item.y); const b = scale(item.x2, item.y2); return { ...item, x: a.x, y: a.y, x2: b.x, y2: b.y, points: item.points?.map((value) => scale(value.x, value.y)) }; });
        }
        dirty.current = true;
        return;
      }

      if (annotationEdit) {
        const world = toWorld(transform.current, point.x, point.y);
        const { original, origin, mode } = annotationEdit;
        const dx = world.x - origin.x; const dy = world.y - origin.y;
        const next: Annotation = mode === 'move'
          ? { ...original, x: original.x + dx, y: original.y + dy, x2: original.x2 + dx, y2: original.y2 + dy, points: original.points?.map((item) => ({ x: item.x + dx, y: item.y + dy })) }
          : (() => {
              const geometry = annotationGeometryBounds(original, transform.current);
              const frame = annotationBounds(original, transform.current);
              const width = Math.max(geometry.maxX - geometry.minX, 1 / transform.current.k);
              const height = Math.max(geometry.maxY - geometry.minY, 1 / transform.current.k);
              const targetX = world.x - (frame.maxX - geometry.maxX);
              const targetY = world.y - (frame.maxY - geometry.maxY);
              const scaleX = Math.max((targetX - geometry.minX) / width, 0.04);
              const scaleY = Math.max((targetY - geometry.minY) / height, 0.04);
              const scalePoint = (x: number, y: number) => ({
                x: geometry.minX + (x - geometry.minX) * scaleX,
                y: geometry.minY + (y - geometry.minY) * scaleY,
              });
              const start = scalePoint(original.x, original.y);
              const end = scalePoint(original.x2, original.y2);
              return {
                ...original,
                x: start.x,
                y: start.y,
                x2: end.x,
                y2: end.y,
                points: original.points?.map((item) => scalePoint(item.x, item.y)),
              };
            })();
        draftAnnotation.current = next;
        dirty.current = true;
        return;
      }

      if (draftAnnotation.current) {
        const drawing = draftAnnotation.current;
        const world = toWorld(transform.current, point.x, point.y);
        drawing.x2 = world.x;
        drawing.y2 = world.y;
        if (drawing.kind === 'pencil') drawing.points?.push(world);
        dirty.current = true;
        return;
      }

      if (dragNodes && dragOrigin) {
        const world = toWorld(transform.current, point.x, point.y);
        const dx = world.x - dragOrigin.x;
        const dy = world.y - dragOrigin.y;
        for (const position of dragPositions) {
          position.node.fx = position.x + dx;
          position.node.fy = position.y + dy;
        }
        moved += Math.abs(event.movementX) + Math.abs(event.movementY);
        placeTooltip(point);
        dirty.current = true;
        return;
      }

      if (selecting && selectionStart) {
        selectionRect = {
          x: Math.min(selectionStart.x, point.x),
          y: Math.min(selectionStart.y, point.y),
          width: Math.abs(point.x - selectionStart.x),
          height: Math.abs(point.y - selectionStart.y),
        };
        marquee.current = selectionRect;
        moved += Math.abs(event.movementX) + Math.abs(event.movementY);
        dirty.current = true;
        return;
      }

      if (panning) {
        transform.current.x += event.movementX;
        transform.current.y += event.movementY;
        moved += Math.abs(event.movementX) + Math.abs(event.movementY);
        dirty.current = true;
        return;
      }

      const world = toWorld(transform.current, point.x, point.y);
      const selectedAnnotation = live.current.selectedAnnotationIds.size === 1
        ? live.current.annotations.find((item) => live.current.selectedAnnotationIds.has(item.id))
        : null;
      if (selectedAnnotation) {
        const box = annotationBounds(selectedAnnotation, transform.current);
        const handle = 20 / transform.current.k;
        if (Math.abs(world.x - box.maxX) <= handle && Math.abs(world.y - box.maxY) <= handle) {
          canvas.style.cursor = 'nwse-resize';
          return;
        }
        if (world.x >= box.minX && world.x <= box.maxX && world.y >= box.minY && world.y <= box.maxY) {
          canvas.style.cursor = 'move';
          return;
        }
      }
      const node = findNodeAt(live.current.model, world, transform.current.k);
      const link = node ? null : findLinkAt(live.current.model, world, transform.current.k);
      placeTooltip(point);

      if (node === hoveredNode.current && link === hoveredLink.current) return;
      hoveredNode.current = node;
      hoveredLink.current = link;
      dirty.current = true;
      canvas.style.cursor = node ? 'pointer' : link ? 'crosshair' : 'default';
      setHover(node ? { type: 'node', node } : link ? { type: 'link', link } : null);
    };

    const onPointerUp = (event: PointerEvent) => {
      canvas.releasePointerCapture?.(event.pointerId);
      const wasClick = moved < CLICK_SLOP;

      if (groupEdit) {
        const preview = groupPreview.current;
        if (preview) onAnnotationsChange(live.current.annotations.map((item) => preview.find((candidate) => candidate.id === item.id) ?? item));
        groupPreview.current = null;
        groupEdit = null;
        dirty.current = true;
        return;
      }

      if (annotationEdit) {
        const edited = draftAnnotation.current;
        if (edited) onAnnotationsChange(live.current.annotations.map((item) => item.id === edited.id ? edited : item));
        draftAnnotation.current = null;
        annotationEdit = null;
        dirty.current = true;
        return;
      }

      if (draftAnnotation.current) {
        const finished = draftAnnotation.current;
        draftAnnotation.current = null;
        if (finished.kind === 'text') {
          setTextEditor({ x: finished.x, y: finished.y });
          window.requestAnimationFrame(() => textInputRef.current?.focus({ preventScroll: true }));
          dirty.current = true;
          return;
        }
        onAnnotationsChange([...live.current.annotations, finished]);
        dirty.current = true;
        return;
      }

      if (dragNodes) {
        if (!frozen) simulation.current?.alphaTarget(0);

        if (wasClick) {
          for (const node of dragNodes) {
            node.fx = null;
            node.fy = null;
          }
          onSelect(dragNodes[0].id, event.shiftKey);
        }
        dragNodes = null;
        dragOrigin = null;
        dragPositions = [];
      } else if (selecting) {
        if (wasClick) {
          onSelect(null);
        } else if (selectionRect) {
          const start = toWorld(transform.current, selectionRect.x, selectionRect.y);
          const end = toWorld(transform.current, selectionRect.x + selectionRect.width, selectionRect.y + selectionRect.height);
          const annotationIds = new Set(
            live.current.annotations
              .filter((item) => {
                const box = annotationBounds(item, transform.current);
                return box.minX >= start.x && box.maxX <= end.x && box.minY >= start.y && box.maxY <= end.y;
              })
              .map((item) => item.id),
          );
          if (annotationIds.size > 0) {
            setSelectedAnnotationIds(annotationIds);
            selecting = false;
            selectionStart = null;
            selectionRect = null;
            marquee.current = null;
            panning = false;
            downAt = null;
            canvas.style.cursor = 'default';
            dirty.current = true;
            return;
          }
          const ids = new Set(
            live.current.model.nodes
              .filter((node) => node.x >= start.x && node.x <= end.x && node.y >= start.y && node.y <= end.y)
              .map((node) => node.id),
          );
          onSelectMany(ids);
        }
        selecting = false;
        selectionStart = null;
        selectionRect = null;
        marquee.current = null;
      } else if (wasClick && downAt) {
        const world = toWorld(transform.current, downAt.x, downAt.y);
        const link = findLinkAt(live.current.model, world, transform.current.k);
        onSelectLink(link?.id ?? null);
      }

      panning = false;
      downAt = null;
      canvas.style.cursor = hoveredNode.current ? 'pointer' : 'default';
      dirty.current = true;
    };

    const onDoubleClick = (event: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      const world = toWorld(transform.current, event.clientX - rect.left, event.clientY - rect.top);
      const node = findNodeAt(live.current.model, world, transform.current.k);
      if (!node) return;
      node.fx = null;
      node.fy = null;
      if (!frozen) simulation.current?.alpha(0.3).restart();
      dirty.current = true;
    };

    const onPointerLeave = () => {
      if (!hoveredNode.current && !hoveredLink.current) return;
      hoveredNode.current = null;
      hoveredLink.current = null;
      setHover(null);
      dirty.current = true;
    };

    canvas.addEventListener('wheel', onWheel, { passive: false });
    canvas.addEventListener('pointerdown', onPointerDown);
    canvas.addEventListener('pointermove', onPointerMove);
    canvas.addEventListener('pointerup', onPointerUp);
    canvas.addEventListener('pointercancel', onPointerUp);
    canvas.addEventListener('pointerleave', onPointerLeave);
    canvas.addEventListener('dblclick', onDoubleClick);
    return () => {
      canvas.removeEventListener('wheel', onWheel);
      canvas.removeEventListener('pointerdown', onPointerDown);
      canvas.removeEventListener('pointermove', onPointerMove);
      canvas.removeEventListener('pointerup', onPointerUp);
      canvas.removeEventListener('pointercancel', onPointerUp);
      canvas.removeEventListener('pointerleave', onPointerLeave);
      canvas.removeEventListener('dblclick', onDoubleClick);
    };
  }, [frozen, onAnnotationsChange, onSelect, onSelectLink, onSelectMany]);

  useImperativeHandle(
    handle,
    (): GraphHandle => ({
      fit() {
        fitMode.current = 'once';
        dirty.current = true;
      },
      zoomBy(factor: number) {
        fitMode.current = 'off';
        const t = transform.current;
        const cx = size.current.width / 2;
        const cy = size.current.height / 2;
        const k = clamp(t.k * factor, MIN_ZOOM, MAX_ZOOM);
        t.x = cx - ((cx - t.x) / t.k) * k;
        t.y = cy - ((cy - t.y) / t.k) * k;
        t.k = k;
        dirty.current = true;
      },
      centerOn(id: string) {
        const node = live.current.model.byId.get(id);
        if (!node) return;
        fitMode.current = 'off';
        const t = transform.current;
        t.k = clamp(Math.max(t.k, 1.1), MIN_ZOOM, MAX_ZOOM);
        t.x = size.current.width / 2 - node.x * t.k;
        t.y = size.current.height / 2 - node.y * t.k;
        dirty.current = true;
      },
      reheat() {
        simulation.current?.alpha(0.9).restart();
        dirty.current = true;
      },
      unpinAll() {
        for (const node of live.current.model.nodes) {
          node.fx = null;
          node.fy = null;
        }
        simulation.current?.alpha(0.5).restart();
        dirty.current = true;
      },
      exportPng(filename: string) {
        exportPng(live.current, filename);
      },
    }),
    [handle],
  );

  return (
    <div
      ref={containerRef}
      className="absolute bottom-[var(--sheet-h)] left-[var(--investigation-w)] right-[var(--inspector-w)] top-0 z-0 block isolate overflow-hidden touch-none bg-plane"
    >
      <canvas ref={canvasRef} className="absolute inset-0 block size-full touch-none" />
      {textEditor && (
        <input
          ref={textInputRef}
          className="absolute z-20 min-w-[150px] border-b border-accent bg-surface-1 px-1 py-0.5 text-xs text-text-primary outline-none"
          style={{ left: textEditor.x * transform.current.k + transform.current.x, top: textEditor.y * transform.current.k + transform.current.y }}
          placeholder="Annotation text"
          onKeyDown={(event) => {
            if (event.key === 'Escape') setTextEditor(null);
            if (event.key === 'Enter' && event.currentTarget.value.trim()) {
              event.currentTarget.dataset.committed = 'true';
              onAnnotationsChange([...live.current.annotations, { id: crypto.randomUUID(), kind: 'text', x: textEditor.x, y: textEditor.y, x2: textEditor.x, y2: textEditor.y, text: event.currentTarget.value.trim(), ...live.current.annotationStyle }]);
              setTextEditor(null);
            }
          }}
          onBlur={(event) => {
            if (event.currentTarget.value.trim() && event.currentTarget.dataset.committed !== 'true') onAnnotationsChange([...live.current.annotations, { id: crypto.randomUUID(), kind: 'text', x: textEditor.x, y: textEditor.y, x2: textEditor.x, y2: textEditor.y, text: event.currentTarget.value.trim(), ...live.current.annotationStyle }]);
            setTextEditor(null);
          }}
        />
      )}
      <div
        ref={tooltipRef}
        className="pointer-events-none absolute left-0 top-0 z-15 min-w-[176px] max-w-[260px] rounded-ui border border-hairline-strong bg-surface-1 px-[11px] py-[9px] opacity-0 shadow-pop transition-opacity duration-90 data-[visible=true]:opacity-100"
        role="tooltip"
        aria-hidden={hover === null}
        data-visible={hover !== null}
      >
        {hover?.type === 'node' && <NodeTip node={hover.node} />}
        {hover?.type === 'link' && <LinkTip link={hover.link} />}
      </div>
    </div>
  );
}

const KIND_LABEL: Record<GraphNode['kind'], string> = {
  focus: 'Focus wallet',
  contract: 'Contract',
  eoa: 'Wallet',
};

const TONE_LABEL: Record<GraphLink['tone'], string> = {
  eth: 'ETH transfers',
  token: 'Token transfers',
  mixed: 'ETH + token transfers',
  call: 'Contract interactions',
};

const TIP_ASSETS = 4;

function NodeTip({ node }: { node: GraphNode }) {
  const tokens = netFlow(node).filter((flow) => !flow.native && flow.amount !== 0n);
  const systemLabel = getPrimarySystemLabel(node.systemLabels);

  return (
    <>
      <div className="flex items-center gap-[6px]">
        <Dot tone={node.kind} />
        <span className="text-[10px] uppercase tracking-[0.07em] text-text-muted">
          {KIND_LABEL[node.kind]}
        </span>
      </div>
      {systemLabel && (
        <div className="mt-[3px] max-w-[260px] break-words text-[13px] font-semibold text-text-primary">
          {systemLabel.value}
        </div>
      )}
      <div className="mb-[7px] mt-[3px] break-all font-mono-ui text-xs text-text-secondary">
        {shortAddress(node.id, 12, 8)}
      </div>
      <dl className="m-0 grid gap-[3px] text-xs [&>div]:flex [&>div]:justify-between [&>div]:gap-3 [&_dd]:m-0 [&_dd]:tabular-nums [&_dt]:text-text-muted">
        <div>
          <dt>In</dt>
          <dd>
            {formatEth(node.valueIn)} ETH · {formatCount(node.inCount)} tx
          </dd>
        </div>
        <div>
          <dt>Out</dt>
          <dd>
            {formatEth(node.valueOut)} ETH · {formatCount(node.outCount)} tx
          </dd>
        </div>
        <div>
          <dt>Peers</dt>
          <dd>{formatCount(node.degree)}</dd>
        </div>
      </dl>
      {tokens.length > 0 && (
        <dl className="m-0 mt-[7px] grid gap-[3px] border-t border-hairline pt-[7px] text-xs [&>div]:flex [&>div]:justify-between [&>div]:gap-3 [&_dd]:m-0 [&_dd]:tabular-nums [&_dt]:font-mono-ui [&_dt]:text-series-token">
          {tokens.slice(0, TIP_ASSETS).map((flow) => (
            <div key={flow.key}>
              <dt>{assetLabel(flow)}</dt>
              <dd>
                {flow.amount > 0n ? '+' : ''}
                {formatUnits(flow.amount, flow.decimals)}
              </dd>
            </div>
          ))}
          {tokens.length > TIP_ASSETS && (
            <div>
              <dt>…</dt>
              <dd>+{formatCount(tokens.length - TIP_ASSETS)} more</dd>
            </div>
          )}
        </dl>
      )}
    </>
  );
}

function LinkTip({ link }: { link: GraphLink }) {
  return (
    <>
      <div className="flex items-center gap-[6px]">
        <Dot tone={link.tone === 'call' ? 'eoa' : 'token'} />
        <span className="text-[10px] uppercase tracking-[0.07em] text-text-muted">
          {TONE_LABEL[link.tone]}
        </span>
      </div>
      <div className="mb-[7px] mt-[3px] break-all font-mono-ui text-xs">
        {shortAddress(link.source.id, 8, 4)} → {shortAddress(link.target.id, 8, 4)}
      </div>
      <dl className="m-0 grid gap-[3px] text-xs [&>div]:flex [&>div]:justify-between [&>div]:gap-3 [&_dd]:m-0 [&_dd]:tabular-nums [&_dt]:text-text-muted">
        {link.assets.slice(0, TIP_ASSETS).map((flow) => (
          <div key={flow.key}>
            <dt>{assetLabel(flow)}</dt>
            <dd>
              {formatUnits(flow.amount, flow.decimals)} · {formatCount(flow.count)} tx
            </dd>
          </div>
        ))}
        {link.assets.length > TIP_ASSETS && (
          <div>
            <dt>…</dt>
            <dd>+{formatCount(link.assets.length - TIP_ASSETS)} assets</dd>
          </div>
        )}
        {link.calls > 0 && (
          <div>
            <dt>Calls</dt>
            <dd>{formatCount(link.calls)}</dd>
          </div>
        )}
        {link.failed > 0 && (
          <div>
            <dt>Reverted</dt>
            <dd>{formatCount(link.failed)}</dd>
          </div>
        )}
      </dl>
    </>
  );
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function fitTarget(
  model: GraphModel,
  size: { width: number; height: number },
): Transform | null {
  const bounds = contentBounds(model);
  if (!bounds || size.width === 0) return null;
  const padding = 72;
  const width = Math.max(bounds.maxX - bounds.minX, 1);
  const height = Math.max(bounds.maxY - bounds.minY, 1);
  const k = clamp(
    Math.min((size.width - padding * 2) / width, (size.height - padding * 2) / height),
    MIN_ZOOM,
    1.6,
  );
  return {
    k,
    x: size.width / 2 - ((bounds.minX + bounds.maxX) / 2) * k,
    y: size.height / 2 - ((bounds.minY + bounds.maxY) / 2) * k,
  };
}

function ease(current: Transform, goal: Transform | null): boolean {
  if (!goal) return true;
  const rate = 0.16;
  current.x += (goal.x - current.x) * rate;
  current.y += (goal.y - current.y) * rate;
  current.k += (goal.k - current.k) * rate;
  const close =
    Math.abs(goal.x - current.x) < 0.5 &&
    Math.abs(goal.y - current.y) < 0.5 &&
    Math.abs(goal.k - current.k) < 0.001;
  if (close) {
    current.x = goal.x;
    current.y = goal.y;
    current.k = goal.k;
  }
  return close;
}

function exportPng(
  state: {
    model: GraphModel;
    palette: VizPalette;
    searchMatches: Set<string>;
    labelMode: LabelMode;
    selectedId: string | null;
  },
  filename: string,
): void {
  const bounds = contentBounds(state.model);
  if (!bounds) return;

  const scale = 2;
  const padding = 64;
  const width = Math.min(3600, Math.max(1200, bounds.maxX - bounds.minX + padding * 2));
  const height = Math.min(3600, Math.max(800, bounds.maxY - bounds.minY + padding * 2));

  const canvas = document.createElement('canvas');
  canvas.width = Math.round(width * scale);
  canvas.height = Math.round(height * scale);
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const transform = fitTarget(state.model, { width, height });
  if (!transform) return;

  drawScene(ctx, {
    model: state.model,
    palette: state.palette,
    transform,
    width,
    height,
    dpr: scale,
    activeId: null,
    selectedId: state.selectedId,
    highlightNodes: null,
    highlightLinks: null,
    searchMatches: state.searchMatches,
    focusMix: 0,
    labelMode: state.labelMode === 'none' ? 'none' : 'auto',
    time: 0,
    showGrid: false,
    showFlow: false,
  });

  canvas.toBlob((blob) => {
    if (!blob) return;
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    URL.revokeObjectURL(url);
  }, 'image/png');
}
