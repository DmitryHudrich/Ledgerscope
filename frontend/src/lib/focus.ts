import type { GraphModel } from '../graph/model';

export type FocusRule = 'roots' | 'connected' | 'shared' | 'selection' | 'groups' | 'labeled' | 'search';
export type LinkRule = 'both' | 'either';
export type FocusScope = 'rules' | 'chosen';

export interface FocusSettings {
  enabled: boolean;
  scope: FocusScope;
  rules: FocusRule[];
  connectedMin: number;
  sharedMin: number;
  opacity: number;
  blur: number;
  links: LinkRule;
}

export const DEFAULT_FOCUS: FocusSettings = {
  enabled: false,
  scope: 'rules',
  rules: ['roots', 'connected'],
  connectedMin: 2,
  sharedMin: 2,
  opacity: 0.18,
  blur: 0,
  links: 'both',
};

export const FOCUS_RULES: Array<{ rule: FocusRule; title: string; hint: string }> = [
  { rule: 'roots', title: 'Roots', hint: 'The addresses you investigate' },
  { rule: 'connected', title: 'Intersections', hint: 'Linked to at least N other nodes on the canvas' },
  { rule: 'shared', title: 'Shared by roots', hint: 'Counterparty of at least N roots' },
  { rule: 'selection', title: 'Selection', hint: 'Selected nodes and their direct neighbours' },
  { rule: 'groups', title: 'Groups', hint: 'Members of any visible group' },
  { rule: 'labeled', title: 'Labeled', hint: 'Nodes with any label' },
  { rule: 'search', title: 'Search matches', hint: 'Nodes matching the filter search' },
];

export interface FocusContext {
  roots: Set<string>;
  selected: Set<string>;
  grouped: Set<string>;
  searchMatches: Set<string>;
  dimmed: Set<string>;
}

export interface FocusResult {
  nodes: Set<string>;
  links: Set<string>;
}

export function focusSets(model: GraphModel, settings: FocusSettings, context: FocusContext): FocusResult | null {
  if (!settings.enabled || model.nodes.length === 0) return null;

  if (settings.scope === 'chosen') {
    if (context.dimmed.size === 0) return null;
    const bright = new Set(model.nodes.filter((node) => !context.dimmed.has(node.id)).map((node) => node.id));
    return { nodes: bright, links: brightLinks(model, bright, settings.links) };
  }

  const rules = new Set(settings.rules);
  const bright = new Set<string>(context.selected);

  for (const node of model.nodes) {
    const id = node.id;
    const neighbors = model.neighbors.get(id) ?? new Set<string>();
    const keep =
      (rules.has('roots') && context.roots.has(id))
      || (rules.has('connected') && neighbors.size >= settings.connectedMin)
      || (rules.has('shared') && !context.roots.has(id) && countIn(neighbors, context.roots) >= settings.sharedMin)
      || (rules.has('groups') && context.grouped.has(id))
      || (rules.has('labeled') && node.systemLabels.some((label) => label.value.trim().length > 0))
      || (rules.has('search') && context.searchMatches.has(id));
    if (keep) bright.add(id);
  }

  if (rules.has('selection')) {
    for (const id of context.selected) {
      if (!model.byId.has(id)) continue;
      bright.add(id);
      for (const neighbor of model.neighbors.get(id) ?? []) bright.add(neighbor);
    }
  }

  return { nodes: bright, links: brightLinks(model, bright, settings.links) };
}

function brightLinks(model: GraphModel, bright: Set<string>, rule: LinkRule): Set<string> {
  const links = new Set<string>();
  for (const link of model.links) {
    const source = bright.has(link.source.id);
    const target = bright.has(link.target.id);
    if (rule === 'both' ? source && target : source || target) links.add(link.id);
  }
  return links;
}

function countIn(values: Set<string>, pool: Set<string>): number {
  let count = 0;
  for (const value of values) if (pool.has(value)) count += 1;
  return count;
}

const STORAGE_KEY = 'ledgerscope.focus';

export function readFocus(): FocusSettings {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
    if (!raw || typeof raw !== 'object') return DEFAULT_FOCUS;
    const known = new Set(FOCUS_RULES.map((entry) => entry.rule));
    return {
      ...DEFAULT_FOCUS,
      ...raw,
      enabled: false,
      scope: raw.scope === 'chosen' ? 'chosen' : 'rules',
      rules: Array.isArray(raw.rules) ? raw.rules.filter((rule: unknown) => known.has(rule as FocusRule)) : DEFAULT_FOCUS.rules,
    };
  } catch {
    return DEFAULT_FOCUS;
  }
}

export function writeFocus(settings: FocusSettings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    return;
  }
}
