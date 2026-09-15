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
  searchMatches: Set<string>;
  labelMode: LabelMode;
  showGrid: boolean;
  showFlow: boolean;
  frozen: boolean;
  onSelect: (id: string | null) => void;
  handle: RefObject<GraphHandle | null>;
}

const MIN_ZOOM = 0.04;
const MAX_ZOOM = 6;
const CLICK_SLOP = 4;

export function GraphCanvas({
  model,
  palette,
  selectedId,
  searchMatches,
  labelMode,
  showGrid,
  showFlow,
  frozen,
  onSelect,
  handle,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);

  const transform = useRef<Transform>({ x: 0, y: 0, k: 1 });
  const size = useRef({ width: 0, height: 0, dpr: 1 });
  const simulation = useRef<Simulation<GraphNode, GraphLink> | null>(null);

  const hoveredNode = useRef<GraphNode | null>(null);
  const hoveredLink = useRef<GraphLink | null>(null);
  const focusMix = useRef(0);
  const retained = useRef<{ nodes: Set<string>; links: Set<string> } | null>(null);
  const dirty = useRef(true);

  const fitMode = useRef<'off' | 'once'>('once');

  const [hover, setHover] = useState<Hover | null>(null);

  const live = useRef({ model, palette, selectedId, searchMatches, labelMode, showGrid, showFlow });
  live.current = { model, palette, selectedId, searchMatches, labelMode, showGrid, showFlow };
  useEffect(() => {
    dirty.current = true;
  }, [palette, selectedId, searchMatches, labelMode, showGrid, showFlow]);

  useEffect(() => {
    const nodes = model.nodes;
    hoveredNode.current = null;
    hoveredLink.current = null;
    setHover(null);

    if (nodes.length === 0) {
      simulation.current?.stop();
      simulation.current = null;
      dirty.current = true;
      return;
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
    const budget = nodes.length > 1200 ? 90 : nodes.length > 400 ? 170 : 320;
    for (let i = 0; i < budget && sim.alpha() > sim.alphaMin(); i += 1) sim.tick();

    simulation.current = sim;
    sim.on('tick', () => {
      dirty.current = true;
    });

    fitMode.current = 'once';
    dirty.current = true;

    return () => {
      sim.on('tick', null);
      sim.stop();
    };
  }, [model]);

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

    const apply = () => {
      const rect = container.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      size.current = { width: rect.width, height: rect.height, dpr };
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
      dirty.current = true;
    };

    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

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
        highlightNodes: sets ? sets.nodes : null,
        highlightLinks: sets ? sets.links : null,
        searchMatches: state.searchMatches,
        focusMix: focusMix.current,
        labelMode: state.labelMode,
        time,
        showGrid: state.showGrid,
        showFlow: state.showFlow,
      });
    });

    return () => cancelAnimationFrame(frame);
  }, [frozen]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let downAt: { x: number; y: number } | null = null;
    let dragNode: GraphNode | null = null;
    let panning = false;
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
      const node =
        event.button === 0 ? findNodeAt(live.current.model, world, transform.current.k) : null;
      if (node) {
        dragNode = node;
        node.fx = node.x;
        node.fy = node.y;
        if (!frozen) simulation.current?.alphaTarget(0.28).restart();
      } else {
        panning = true;
      }
      canvas.style.cursor = 'grabbing';
    };

    const onPointerMove = (event: PointerEvent) => {
      const point = localPoint(event);

      if (dragNode) {
        const world = toWorld(transform.current, point.x, point.y);
        dragNode.fx = world.x;
        dragNode.fy = world.y;
        moved += Math.abs(event.movementX) + Math.abs(event.movementY);
        placeTooltip(point);
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

      if (dragNode) {
        if (!frozen) simulation.current?.alphaTarget(0);

        if (wasClick) {
          dragNode.fx = null;
          dragNode.fy = null;
          onSelect(dragNode.id);
        }
        dragNode = null;
      } else if (wasClick && downAt) {
        const world = toWorld(transform.current, downAt.x, downAt.y);
        const link = findLinkAt(live.current.model, world, transform.current.k);
        onSelect(link ? link.source.id : null);
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
  }, [frozen, onSelect]);

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
    <div ref={containerRef} className="graph-stage">
      <canvas ref={canvasRef} className="graph-canvas" />
      <div
        ref={tooltipRef}
        className="graph-tooltip"
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

  return (
    <>
      <div className="tip-head">
        <span className={`dot dot-${node.kind}`} aria-hidden="true" />
        <span className="tip-kind">{KIND_LABEL[node.kind]}</span>
      </div>
      <div className="tip-address">{shortAddress(node.id, 12, 8)}</div>
      <dl className="tip-rows">
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
        <dl className="tip-rows tip-assets">
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
      <div className="tip-head">
        <span className={`dot dot-${link.tone === 'call' ? 'eoa' : 'token'}`} aria-hidden="true" />
        <span className="tip-kind">{TONE_LABEL[link.tone]}</span>
      </div>
      <div className="tip-address">
        {shortAddress(link.source.id, 8, 4)} → {shortAddress(link.target.id, 8, 4)}
      </div>
      <dl className="tip-rows">
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
