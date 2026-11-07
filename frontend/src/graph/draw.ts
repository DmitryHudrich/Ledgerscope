import { shortAddress } from '../lib/format';
import { withAlpha, type VizPalette } from '../lib/theme';
import type { GraphLink, GraphModel, GraphNode } from './model';

export interface Transform {
  x: number;
  y: number;
  k: number;
}

export type LabelMode = 'auto' | 'all' | 'none';

export interface Scene {
  model: GraphModel;
  palette: VizPalette;
  transform: Transform;

  width: number;
  height: number;

  dpr: number;
  activeId: string | null;
  selectedId: string | null;
  selectedIds?: Set<string>;
  selectedLinkId?: string | null;
  nodeLabels?: Map<string, string>;
  highlightNodes: Set<string> | null;
  highlightLinks: Set<string> | null;
  searchMatches: Set<string>;

  focusMix: number;
  labelMode: LabelMode;

  time: number;
  showGrid: boolean;
  showFlow: boolean;
  transparentBackground?: boolean;
  groups?: SceneGroup[];
  fade?: SceneFade | null;
}

export interface SceneFade {
  nodes: Set<string>;
  links: Set<string>;
  opacity: number;
  blur: number;
}

let fadeLayer: HTMLCanvasElement | null = null;

function layerFor(width: number, height: number): CanvasRenderingContext2D | null {
  if (typeof document === 'undefined') return null;
  fadeLayer ??= document.createElement('canvas');
  if (fadeLayer.width !== width) fadeLayer.width = width;
  if (fadeLayer.height !== height) fadeLayer.height = height;
  return fadeLayer.getContext('2d');
}

export interface SceneGroup {
  id: string;
  name: string;
  color: string;
  nodes: GraphNode[];
  links: GraphLink[];
}

const DIM = 0.86;
const MAX_LABELS = 90;
const FONT = 'system-ui, -apple-system, "Segoe UI", sans-serif';

export function nodeColor(node: GraphNode, palette: VizPalette): string {
  if (node.kind === 'focus') return palette.focus;
  if (node.kind === 'contract') return palette.contract;
  return palette.eoa;
}

export function linkColor(link: GraphLink, palette: VizPalette, lit: boolean): string {
  if (link.tone === 'token' || link.tone === 'mixed') return palette.token;
  if (link.tone === 'call') return palette.edge;
  return lit ? nodeColor(link.source, palette) : palette.edge;
}

export function toWorld(t: Transform, sx: number, sy: number): { x: number; y: number } {
  return { x: (sx - t.x) / t.k, y: (sy - t.y) / t.k };
}

export function toScreen(t: Transform, wx: number, wy: number): { x: number; y: number } {
  return { x: wx * t.k + t.x, y: wy * t.k + t.y };
}

function controlPoint(link: GraphLink): { cx: number; cy: number } {
  const { source, target } = link;
  const mx = (source.x + target.x) / 2;
  const my = (source.y + target.y) / 2;
  if (!link.curve) return { cx: mx, cy: my };
  const dx = target.x - source.x;
  const dy = target.y - source.y;
  return { cx: mx - dy * link.curve, cy: my + dx * link.curve };
}

export function pointOnLink(link: GraphLink, t: number): { x: number; y: number } {
  const { cx, cy } = controlPoint(link);
  const u = 1 - t;
  return {
    x: u * u * link.source.x + 2 * u * t * cx + t * t * link.target.x,
    y: u * u * link.source.y + 2 * u * t * cy + t * t * link.target.y,
  };
}

function loopGeometry(node: GraphNode): { cx: number; cy: number; r: number } {
  const r = node.r * 0.95 + 7;
  return { cx: node.x, cy: node.y - node.r - r * 0.55, r };
}

export function drawScene(ctx: CanvasRenderingContext2D, scene: Scene): void {
  const { transform: t, palette, model, width, height, dpr } = scene;

  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  if (scene.transparentBackground) {
    ctx.clearRect(0, 0, width, height);
  } else {
    ctx.fillStyle = palette.plane;
    ctx.fillRect(0, 0, width, height);
    if (scene.showGrid) drawGrid(ctx, scene);
  }

  ctx.save();
  ctx.setTransform(dpr * t.k, 0, 0, dpr * t.k, dpr * t.x, dpr * t.y);
  ctx.lineCap = 'round';

  const pad = 80 / t.k;
  const view = {
    minX: -t.x / t.k - pad,
    minY: -t.y / t.k - pad,
    maxX: (width - t.x) / t.k + pad,
    maxY: (height - t.y) / t.k + pad,
  };

  if (scene.groups && scene.groups.length > 0) drawGroups(ctx, scene.groups, scene);

  const fade = scene.fade ?? null;
  const fadedLinks: GraphLink[] = [];
  const fadedNodes: GraphNode[] = [];

  const highlightLinks = scene.highlightLinks;
  const dimmed: GraphLink[] = [];
  const lit: GraphLink[] = [];
  for (const link of model.links) {
    if (!linkInView(link, view)) continue;
    if (fade && !fade.links.has(link.id) && link.id !== scene.selectedLinkId) {
      fadedLinks.push(link);
      continue;
    }
    if ((highlightLinks && highlightLinks.has(link.id)) || link.id === scene.selectedLinkId) lit.push(link);
    else dimmed.push(link);
  }

  const visible: GraphNode[] = [];
  for (const node of model.nodes) {
    if (
      node.x + node.r < view.minX ||
      node.x - node.r > view.maxX ||
      node.y + node.r < view.minY ||
      node.y - node.r > view.maxY
    ) {
      continue;
    }
    const pinned = node.id === scene.activeId;
    if (fade && !fade.nodes.has(node.id) && !pinned) {
      fadedNodes.push(node);
      continue;
    }
    visible.push(node);
  }

  if (fade && (fadedLinks.length > 0 || fadedNodes.length > 0) && fade.opacity > 0.001) {
    const layer = layerFor(Math.round(width * dpr), Math.round(height * dpr));
    if (layer) {
      layer.setTransform(1, 0, 0, 1, 0, 0);
      layer.clearRect(0, 0, layer.canvas.width, layer.canvas.height);
      layer.setTransform(dpr * t.k, 0, 0, dpr * t.k, dpr * t.x, dpr * t.y);
      layer.lineCap = 'round';
      for (const link of fadedLinks) drawLink(layer, link, scene, 1, false);
      for (const node of fadedNodes) drawNode(layer, node, scene, 1);

      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.globalAlpha = fade.opacity;
      if (fade.blur > 0) ctx.filter = `blur(${(fade.blur * dpr).toFixed(1)}px)`;
      ctx.drawImage(layer.canvas, 0, 0);
      ctx.restore();
    }
  }

  const contextAlpha = 1 - DIM * scene.focusMix;
  for (const link of dimmed) drawLink(ctx, link, scene, contextAlpha, false);
  for (const link of lit) drawLink(ctx, link, scene, 1, true);

  if (scene.showFlow && lit.length > 0 && lit.length <= 400) {
    for (const link of lit) drawFlow(ctx, link, scene);
  }

  const highlightNodes = scene.highlightNodes;
  for (const node of visible) {
    if (highlightNodes?.has(node.id)) continue;
    drawNode(ctx, node, scene, highlightNodes ? contextAlpha : 1);
  }
  if (highlightNodes) {
    for (const node of visible) {
      if (highlightNodes.has(node.id)) drawNode(ctx, node, scene, 1);
    }
  }

  drawLabels(ctx, visible, scene);
  ctx.restore();
}

const HULL_PAD = 16;
const HULL_SAMPLES = 10;

function hull(points: Array<[number, number]>): Array<[number, number]> {
  const sorted = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (sorted.length < 3) return sorted;
  const cross = (o: [number, number], a: [number, number], b: [number, number]) =>
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower: Array<[number, number]> = [];
  for (const point of sorted) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], point) <= 0) lower.pop();
    lower.push(point);
  }
  const upper: Array<[number, number]> = [];
  for (let index = sorted.length - 1; index >= 0; index -= 1) {
    const point = sorted[index];
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], point) <= 0) upper.pop();
    upper.push(point);
  }
  upper.pop();
  lower.pop();
  return lower.concat(upper);
}

function drawGroups(ctx: CanvasRenderingContext2D, groups: SceneGroup[], scene: Scene): void {
  const { transform: t } = scene;

  for (const group of groups) {
    ctx.strokeStyle = withAlpha(group.color, 0.32);
    ctx.lineWidth = 7 / t.k;
    for (const link of group.links) {
      ctx.beginPath();
      if (link.selfLoop) {
        const { cx, cy, r } = loopGeometry(link.source);
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
      } else {
        const { cx, cy } = controlPoint(link);
        ctx.moveTo(link.source.x, link.source.y);
        ctx.quadraticCurveTo(cx, cy, link.target.x, link.target.y);
      }
      ctx.stroke();
    }

    if (group.nodes.length === 0) continue;
    const points: Array<[number, number]> = [];
    for (const node of group.nodes) {
      const radius = node.r + HULL_PAD / Math.sqrt(t.k);
      for (let step = 0; step < HULL_SAMPLES; step += 1) {
        const angle = (step / HULL_SAMPLES) * Math.PI * 2;
        points.push([node.x + Math.cos(angle) * radius, node.y + Math.sin(angle) * radius]);
      }
    }
    const outline = hull(points);
    if (outline.length < 3) continue;

    ctx.beginPath();
    ctx.moveTo(outline[0][0], outline[0][1]);
    for (const [x, y] of outline.slice(1)) ctx.lineTo(x, y);
    ctx.closePath();
    ctx.fillStyle = withAlpha(group.color, 0.09);
    ctx.fill();
    ctx.setLineDash([6 / t.k, 4 / t.k]);
    ctx.strokeStyle = withAlpha(group.color, 0.7);
    ctx.lineWidth = 1.4 / t.k;
    ctx.stroke();
    ctx.setLineDash([]);

    const top = outline.reduce((best, point) => (point[1] < best[1] ? point : best), outline[0]);
    const size = 11 / t.k;
    ctx.font = `600 ${size}px ${FONT}`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'bottom';
    ctx.fillStyle = withAlpha(group.color, 0.95);
    ctx.fillText(group.name, top[0], top[1] - 4 / t.k);
  }
}

function linkInView(
  link: GraphLink,
  view: { minX: number; minY: number; maxX: number; maxY: number },
): boolean {
  const minX = Math.min(link.source.x, link.target.x);
  const maxX = Math.max(link.source.x, link.target.x);
  const minY = Math.min(link.source.y, link.target.y);
  const maxY = Math.max(link.source.y, link.target.y);
  return !(maxX < view.minX || minX > view.maxX || maxY < view.minY || minY > view.maxY);
}

function drawGrid(ctx: CanvasRenderingContext2D, scene: Scene): void {
  const { transform: t, palette, width, height } = scene;
  let step = 80;
  while (step * t.k < 40) step *= 2;
  while (step * t.k > 140) step /= 2;

  const startX = Math.floor((-t.x / t.k) / step) * step;
  const startY = Math.floor((-t.y / t.k) / step) * step;
  const endX = (width - t.x) / t.k;
  const endY = (height - t.y) / t.k;

  ctx.save();
  ctx.fillStyle = palette.grid;
  ctx.beginPath();
  for (let wx = startX; wx <= endX; wx += step) {
    for (let wy = startY; wy <= endY; wy += step) {
      const sx = wx * t.k + t.x;
      const sy = wy * t.k + t.y;
      ctx.moveTo(sx + 1.6, sy);
      ctx.arc(sx, sy, 1.6, 0, Math.PI * 2);
    }
  }
  ctx.fill();
  ctx.restore();
}

function drawLink(
  ctx: CanvasRenderingContext2D,
  link: GraphLink,
  scene: Scene,
  alpha: number,
  lit: boolean,
): void {
  if (alpha <= 0.02) return;
  const { transform: t, palette } = scene;
  const color = linkColor(link, palette, lit);
  const width = Math.max(link.width, lit ? 1.6 : 1) / t.k;
  const tinted = !lit && (link.tone === 'token' || link.tone === 'mixed');

  ctx.strokeStyle = withAlpha(color, alpha * (lit ? 0.92 : tinted ? 0.52 : 0.62));
  ctx.lineWidth = width;
  ctx.setLineDash(link.tone === 'call' ? [6 / t.k, 5 / t.k] : []);

  if (link.selfLoop) {
    const { cx, cy, r } = loopGeometry(link.source);
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.stroke();
    ctx.setLineDash([]);
    return;
  }

  const { cx, cy } = controlPoint(link);
  ctx.beginPath();
  ctx.moveTo(link.source.x, link.source.y);
  ctx.quadraticCurveTo(cx, cy, link.target.x, link.target.y);
  ctx.stroke();
  ctx.setLineDash([]);

  let dx = link.target.x - cx;
  let dy = link.target.y - cy;
  const len = Math.hypot(dx, dy);
  if (len < 1e-3) return;
  dx /= len;
  dy /= len;

  const size = Math.min(9, 4.5 + link.width) / t.k;
  const gap = link.target.r + 2.5 / t.k;
  const tipX = link.target.x - dx * gap;
  const tipY = link.target.y - dy * gap;

  ctx.fillStyle = withAlpha(color, alpha * (lit ? 0.95 : 0.6));
  ctx.beginPath();
  ctx.moveTo(tipX, tipY);
  ctx.lineTo(tipX - dx * size - dy * size * 0.52, tipY - dy * size + dx * size * 0.52);
  ctx.lineTo(tipX - dx * size + dy * size * 0.52, tipY - dy * size - dx * size * 0.52);
  ctx.closePath();
  ctx.fill();
}

function drawFlow(ctx: CanvasRenderingContext2D, link: GraphLink, scene: Scene): void {
  if (link.selfLoop) return;
  const { transform: t, palette } = scene;
  const count = Math.min(3, 1 + Math.floor(link.count / 3));
  const period = 2100;
  const color = linkColor(link, palette, true);
  const r = Math.max(1.5, link.width * 0.75) / t.k;

  ctx.fillStyle = withAlpha(color, 0.95);
  for (let i = 0; i < count; i += 1) {
    const phase = ((scene.time / period) + i / count) % 1;
    const point = pointOnLink(link, phase);
    ctx.beginPath();
    ctx.arc(point.x, point.y, r, 0, Math.PI * 2);
    ctx.fill();
  }
}

function drawNode(
  ctx: CanvasRenderingContext2D,
  node: GraphNode,
  scene: Scene,
  alpha: number,
): void {
  if (alpha <= 0.02) return;
  const { palette, transform: t } = scene;
  const color = nodeColor(node, palette);
  const isActive = node.id === scene.activeId;
  const isSelected = scene.selectedIds?.has(node.id) ?? node.id === scene.selectedId;
  const matched = scene.searchMatches.has(node.id);

  if (isActive || isSelected) {
    ctx.shadowColor = withAlpha(color, 0.55);
    ctx.shadowBlur = 18 / t.k;
  }

  ctx.fillStyle = withAlpha(color, alpha);
  traceNodeShape(ctx, node);
  ctx.fill();
  ctx.shadowBlur = 0;

  ctx.strokeStyle = withAlpha(palette.plane, alpha * 0.95);
  ctx.lineWidth = 2 / t.k;
  traceNodeShape(ctx, node);
  ctx.stroke();

  if (node.kind === 'focus') {
    ctx.strokeStyle = withAlpha(color, alpha * 0.75);
    ctx.lineWidth = 1.5 / t.k;
    ctx.beginPath();
    ctx.arc(node.x, node.y, node.r + 4 / t.k, 0, Math.PI * 2);
    ctx.stroke();
  }

  if (matched || isSelected) {
    ctx.strokeStyle = withAlpha(isSelected ? palette.ink : color, matched ? 0.95 : 0.8);
    ctx.lineWidth = 2 / t.k;
    ctx.beginPath();
    ctx.arc(node.x, node.y, node.r + 6 / t.k, 0, Math.PI * 2);
    ctx.stroke();
  }
}

function traceNodeShape(ctx: CanvasRenderingContext2D, node: GraphNode): void {
  ctx.beginPath();
  if (node.kind === 'contract') {
    const s = node.r * 0.92;
    const radius = Math.min(3, s * 0.35);
    ctx.roundRect(node.x - s, node.y - s, s * 2, s * 2, radius);
  } else {
    ctx.arc(node.x, node.y, node.r, 0, Math.PI * 2);
  }
}

interface LabelBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

function drawLabels(
  ctx: CanvasRenderingContext2D,
  visible: GraphNode[],
  scene: Scene,
): void {
  if (scene.labelMode === 'none') return;
  const { transform: t, palette } = scene;
  const fontPx = 11;
  const highlight = scene.highlightNodes;

  const candidates = visible.filter((node) => {
    if (highlight?.has(node.id)) return true;
    if (node.id === scene.selectedId || node.id === scene.activeId) return true;
    if (scene.searchMatches.has(node.id)) return true;
    if (scene.labelMode === 'all') return true;
    if (highlight) return false;

    return node.r * t.k > 11;
  });

  candidates.sort((a, b) => b.r - a.r);
  const taken: LabelBox[] = [];
  ctx.font = `500 ${fontPx / t.k}px ${FONT}`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'top';
  ctx.lineJoin = 'round';

  let drawn = 0;
  for (const node of candidates) {
    if (drawn >= MAX_LABELS) break;
    const text = scene.nodeLabels?.get(node.id) ?? shortAddress(node.id, 6, 4);
    const w = ctx.measureText(text).width;
    const h = fontPx / t.k;
    const x = node.x;
    const y = node.y + node.r + 4 / t.k;
    const box: LabelBox = { x: x - w / 2, y, w, h };
    if (taken.some((other) => overlaps(box, other))) continue;
    taken.push(box);

    const lit = !highlight || highlight.has(node.id);

    ctx.strokeStyle = withAlpha(palette.plane, lit ? 0.9 : 0.5);
    ctx.lineWidth = 3 / t.k;
    ctx.strokeText(text, x, y);
    ctx.fillStyle = withAlpha(
      node.id === scene.selectedId ? palette.ink : palette.inkSecondary,
      lit ? 1 : 1 - DIM * scene.focusMix,
    );
    ctx.fillText(text, x, y);
    drawn += 1;
  }
}

function overlaps(a: LabelBox, b: LabelBox): boolean {
  return !(a.x + a.w < b.x || b.x + b.w < a.x || a.y + a.h < b.y || b.y + b.h < a.y);
}

export function findNodeAt(
  model: GraphModel,
  world: { x: number; y: number },
  scale: number,
): GraphNode | null {
  const slack = 5 / scale;
  let best: GraphNode | null = null;
  let bestDistance = Infinity;
  for (const node of model.nodes) {
    const reach = node.r + slack;
    const dx = node.x - world.x;
    const dy = node.y - world.y;
    const distance = dx * dx + dy * dy;
    if (distance <= reach * reach && distance < bestDistance) {
      best = node;
      bestDistance = distance;
    }
  }
  return best;
}

export function findLinkAt(
  model: GraphModel,
  world: { x: number; y: number },
  scale: number,
): GraphLink | null {
  const tolerance = 6 / scale;
  let best: GraphLink | null = null;
  let bestDistance = tolerance * tolerance;
  for (const link of model.links) {
    if (link.selfLoop) {
      const { cx, cy, r } = loopGeometry(link.source);
      const d = Math.abs(Math.hypot(world.x - cx, world.y - cy) - r);
      if (d * d < bestDistance) {
        best = link;
        bestDistance = d * d;
      }
      continue;
    }

    let previous = pointOnLink(link, 0);
    for (let i = 1; i <= 8; i += 1) {
      const current = pointOnLink(link, i / 8);
      const d = distanceToSegment(world, previous, current);
      if (d < bestDistance) {
        best = link;
        bestDistance = d;
      }
      previous = current;
    }
  }
  return best;
}

function distanceToSegment(
  p: { x: number; y: number },
  a: { x: number; y: number },
  b: { x: number; y: number },
): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lengthSq = dx * dx + dy * dy;
  const t = lengthSq === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / lengthSq));
  const px = a.x + t * dx - p.x;
  const py = a.y + t * dy - p.y;
  return px * px + py * py;
}

export function contentBounds(model: GraphModel): {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
} | null {
  if (model.nodes.length === 0) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const node of model.nodes) {
    minX = Math.min(minX, node.x - node.r);
    minY = Math.min(minY, node.y - node.r);
    maxX = Math.max(maxX, node.x + node.r);
    maxY = Math.max(maxY, node.y + node.r);
  }
  return { minX, minY, maxX, maxY };
}
