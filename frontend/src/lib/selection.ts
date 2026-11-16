import type { GraphModel } from '../graph/model';

export type SelectMode = 'replace' | 'toggle' | 'add' | 'remove' | 'neighborhood';

export function modeOf(event: { shiftKey: boolean; altKey: boolean; ctrlKey: boolean; metaKey: boolean }): SelectMode {
  if (event.altKey) return 'remove';
  if (event.ctrlKey || event.metaKey) return 'neighborhood';
  if (event.shiftKey) return 'toggle';
  return 'replace';
}

export function marqueeModeOf(event: { shiftKey: boolean; altKey: boolean }): SelectMode {
  if (event.altKey) return 'remove';
  if (event.shiftKey) return 'add';
  return 'replace';
}

export function applySelection(
  current: Set<string>,
  ids: Iterable<string>,
  mode: SelectMode,
  model: GraphModel,
): Set<string> {
  const picked = [...ids];
  switch (mode) {
    case 'replace':
      return new Set(picked);
    case 'add':
      return new Set([...current, ...picked]);
    case 'remove': {
      const next = new Set(current);
      for (const id of picked) next.delete(id);
      return next;
    }
    case 'toggle': {
      const next = new Set(current);
      for (const id of picked) {
        if (next.has(id)) next.delete(id);
        else next.add(id);
      }
      return next;
    }
    case 'neighborhood': {
      const next = new Set(current);
      for (const id of picked) {
        next.add(id);
        for (const neighbor of model.neighbors.get(id) ?? []) next.add(neighbor);
      }
      return next;
    }
  }
}

export function grow(model: GraphModel, selected: Set<string>): Set<string> {
  const next = new Set(selected);
  for (const id of selected) {
    for (const neighbor of model.neighbors.get(id) ?? []) next.add(neighbor);
  }
  return next;
}

export function shrink(model: GraphModel, selected: Set<string>): Set<string> {
  return new Set(
    [...selected].filter((id) => [...(model.neighbors.get(id) ?? [])].every((neighbor) => selected.has(neighbor))),
  );
}

export function neighborsOnly(model: GraphModel, selected: Set<string>): Set<string> {
  const next = new Set<string>();
  for (const id of selected) {
    for (const neighbor of model.neighbors.get(id) ?? []) {
      if (!selected.has(neighbor)) next.add(neighbor);
    }
  }
  return next;
}

export function component(model: GraphModel, selected: Set<string>): Set<string> {
  const seen = new Set<string>();
  const queue = [...selected].filter((id) => model.byId.has(id));
  while (queue.length > 0) {
    const id = queue.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const neighbor of model.neighbors.get(id) ?? []) {
      if (!seen.has(neighbor)) queue.push(neighbor);
    }
  }
  return seen;
}

export function invert(model: GraphModel, selected: Set<string>): Set<string> {
  return new Set(model.nodes.map((node) => node.id).filter((id) => !selected.has(id)));
}

export const SELECTION_SHORTCUTS: Array<{ keys: string; action: string }> = [
  { keys: 'Click', action: 'Select one node' },
  { keys: 'Shift+Click', action: 'Add or remove one node' },
  { keys: 'Ctrl+Click', action: 'Add the node and everyone it talks to' },
  { keys: 'Alt+Click', action: 'Remove the node from the selection' },
  { keys: 'Right drag', action: 'Select in a box (Shift adds, Alt removes)' },
  { keys: 'E', action: 'Grow the selection by one hop' },
  { keys: 'Shift+E', action: 'Shrink it to nodes not on its edge' },
  { keys: 'N', action: 'Keep only the neighbors of the selection' },
  { keys: 'C', action: 'Select the whole connected cluster' },
  { keys: 'I', action: 'Invert the selection' },
  { keys: 'Ctrl+A', action: 'Select every node' },
  { keys: 'Esc', action: 'Clear the selection' },
  { keys: 'Delete', action: 'Hide the selected nodes' },
  { keys: 'Ctrl+F', action: 'Find a wallet on the canvas' },
  { keys: '?', action: 'Show these shortcuts' },
];
