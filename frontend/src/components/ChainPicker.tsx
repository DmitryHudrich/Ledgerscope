import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';

import type { ChainInfo } from '../api/types';
import { IconChevron, IconSearch } from './Icons';
import { Panel, cx, focusRing } from './ui';

interface Props {
  chains: ChainInfo[];
  chain: ChainInfo | null;
  defaultChain: number | null;
  disabled: boolean;
  onChange: (chainId: number) => void;
  onOpen: () => void;
}

type SortKey = 'name' | 'id' | 'rpc';
type Tone = 'l1' | 'l2' | 'tron' | 'alt';

interface Placement {
  ecosystem: string;
  layer: 'l1' | 'l2';
}

interface Ecosystem {
  key: string;
  title: string;
  tone: Tone;
  total: number;
}

interface Group {
  key: string;
  title: string;
  tone: Tone;
  chains: ChainInfo[];
}

const SORT_STORAGE_KEY = 'ledgerscope.chain-sort';
const ECOSYSTEM_STORAGE_KEY = 'ledgerscope.chain-ecosystems';
const OTHER_EVM = 'evm:other';

const FAMILY_TITLES: Record<string, string> = {
  tron: 'Tron',
  ton: 'TON',
};

const SORTS: Array<{ key: SortKey; label: string }> = [
  { key: 'name', label: 'Name' },
  { key: 'id', label: 'Chain ID' },
  { key: 'rpc', label: 'RPC' },
];

const toneGlyph: Record<Tone, string> = {
  l1: 'bg-[color-mix(in_srgb,var(--series-eoa)_16%,transparent)] text-series-eoa',
  l2: 'bg-[color-mix(in_srgb,var(--series-token)_16%,transparent)] text-series-token',
  tron: 'bg-[color-mix(in_srgb,var(--series-focus)_16%,transparent)] text-series-focus',
  alt: 'bg-[color-mix(in_srgb,var(--series-contract)_16%,transparent)] text-series-contract',
};

const toneDot: Record<Tone, string> = {
  l1: 'bg-series-eoa',
  l2: 'bg-series-token',
  tron: 'bg-series-focus',
  alt: 'bg-series-contract',
};

function readSort(): SortKey {
  try {
    const stored = localStorage.getItem(SORT_STORAGE_KEY);
    if (stored === 'name' || stored === 'id' || stored === 'rpc') return stored;
  } catch {
    return 'name';
  }
  return 'name';
}

function writeSort(sort: SortKey) {
  try {
    localStorage.setItem(SORT_STORAGE_KEY, sort);
  } catch {
    return;
  }
}

function readEcosystems(): Set<string> {
  try {
    const stored = JSON.parse(localStorage.getItem(ECOSYSTEM_STORAGE_KEY) ?? '[]');
    return new Set(Array.isArray(stored) ? stored.filter((value) => typeof value === 'string') : []);
  } catch {
    return new Set();
  }
}

function writeEcosystems(ecosystems: Set<string>) {
  try {
    localStorage.setItem(ECOSYSTEM_STORAGE_KEY, JSON.stringify([...ecosystems]));
  } catch {
    return;
  }
}

function toneOf(chain: ChainInfo): Tone {
  if (chain.family === 'tron') return 'tron';
  if (chain.family !== 'evm') return 'alt';
  return chain.l2_of == null ? 'l1' : 'l2';
}

function familyTitle(family: string): string {
  return FAMILY_TITLES[family] ?? family.charAt(0).toUpperCase() + family.slice(1);
}

export function chainGlyph(chain: ChainInfo | null): string {
  const symbol = chain?.native_symbol ?? 'ETH';
  return symbol.toUpperCase() === 'ETH' ? 'Ξ' : symbol.slice(0, 1).toUpperCase();
}

function comparator(sort: SortKey, defaultChain: number | null) {
  return (a: ChainInfo, b: ChainInfo) =>
    Number(b.chain_id === defaultChain) - Number(a.chain_id === defaultChain)
    || (sort === 'id' ? a.chain_id - b.chain_id : 0)
    || (sort === 'rpc' ? b.rpc_nodes - a.rpc_nodes : 0)
    || Number(b.rpc_nodes > 0) - Number(a.rpc_nodes > 0)
    || a.name.localeCompare(b.name);
}

function matches(chain: ChainInfo, query: string): boolean {
  if (!query) return true;
  return [chain.name, chain.slug, chain.native_symbol, String(chain.chain_id)]
    .some((value) => value.toLowerCase().includes(query));
}

function classify(chains: ChainInfo[], defaultChain: number | null) {
  const names = new Map(chains.map((chain) => [chain.chain_id, chain.name]));
  const parents = new Set(chains.flatMap((chain) => (chain.family === 'evm' && chain.l2_of != null ? [chain.l2_of] : [])));
  const placements = new Map<number, Placement>();
  const ecosystems = new Map<string, Ecosystem>();

  for (const chain of chains) {
    const tone = toneOf(chain);
    const layer = tone === 'l2' ? 'l2' : 'l1';
    let key: string;
    let title: string;
    if (chain.family !== 'evm') {
      key = chain.family;
      title = familyTitle(chain.family);
    } else {
      const root = chain.l2_of ?? chain.chain_id;
      const own = parents.has(root) || root === defaultChain;
      key = own ? `evm:${root}` : OTHER_EVM;
      title = own ? (names.get(root) ?? `Chain ${root}`) : 'Other EVM';
    }

    placements.set(chain.chain_id, { ecosystem: key, layer });
    const ecosystem = ecosystems.get(key) ?? {
      key,
      title,
      tone: tone === 'l2' ? 'l1' : tone,
      total: 0,
    };
    ecosystem.total += 1;
    ecosystems.set(key, ecosystem);
  }

  const defaultEcosystem = defaultChain === null ? null : placements.get(defaultChain)?.ecosystem;
  const ordered = [...ecosystems.values()].sort(
    (a, b) =>
      Number(b.key === defaultEcosystem) - Number(a.key === defaultEcosystem)
      || Number(a.key === OTHER_EVM) - Number(b.key === OTHER_EVM)
      || b.total - a.total
      || a.title.localeCompare(b.title),
  );

  return { placements, ecosystems: ordered };
}

function groupsOf(
  chains: ChainInfo[],
  placements: Map<number, Placement>,
  ecosystems: Ecosystem[],
  visible: Set<string>,
  sort: SortKey,
  defaultChain: number | null,
  query: string,
): Group[] {
  const groups: Group[] = ecosystems
    .filter((ecosystem) => visible.size === 0 || visible.has(ecosystem.key))
    .flatMap((ecosystem) => [
      { key: `${ecosystem.key}:l1`, title: ecosystem.title, tone: ecosystem.tone, chains: [] },
      { key: `${ecosystem.key}:l2`, title: `${ecosystem.title} L2s`, tone: 'l2' as Tone, chains: [] },
    ]);
  const byKey = new Map(groups.map((group) => [group.key, group]));

  for (const chain of chains) {
    if (!matches(chain, query)) continue;
    const placement = placements.get(chain.chain_id);
    if (!placement) continue;
    byKey.get(`${placement.ecosystem}:${placement.layer}`)?.chains.push(chain);
  }

  const order = comparator(sort, defaultChain);
  return groups
    .filter((group) => group.chains.length > 0)
    .map((group) => ({ ...group, chains: group.chains.sort(order) }));
}

function chip(on: boolean): string {
  return cx(
    'inline-flex h-6 cursor-pointer items-center gap-1.5 rounded-full border px-2.5 text-[11px] font-medium [transition:background_120ms_ease,border-color_120ms_ease,color_120ms_ease]',
    on
      ? 'border-[color-mix(in_srgb,var(--accent)_45%,transparent)] bg-[color-mix(in_srgb,var(--accent)_14%,transparent)] text-accent'
      : 'border-hairline bg-plane text-text-secondary hover:border-hairline-strong hover:text-text-primary',
    focusRing,
  );
}

export function ChainPicker({ chains, chain, defaultChain, disabled, onChange, onOpen }: Props) {
  const root = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<SortKey>(readSort);
  const [active, setActive] = useState<number | null>(null);
  const [visible, setVisible] = useState<Set<string>>(readEcosystems);

  const needle = query.trim().toLowerCase();
  const { placements, ecosystems } = useMemo(() => classify(chains, defaultChain), [chains, defaultChain]);
  const known = useMemo(() => new Set(ecosystems.map((ecosystem) => ecosystem.key)), [ecosystems]);
  const shown = useMemo(() => new Set([...visible].filter((key) => known.has(key))), [visible, known]);
  const hits = useMemo(() => {
    const counts = new Map<string, number>();
    for (const option of chains) {
      if (!matches(option, needle)) continue;
      const key = placements.get(option.chain_id)?.ecosystem;
      if (key) counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  }, [chains, needle, placements]);
  const groups = useMemo(
    () => groupsOf(chains, placements, ecosystems, shown, sort, defaultChain, needle),
    [chains, placements, ecosystems, shown, sort, defaultChain, needle],
  );
  const flat = useMemo(() => groups.flatMap((group) => group.chains), [groups]);
  const tone = chain ? toneOf(chain) : 'l1';
  const selectable = chains.length > 1 && !disabled;

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  }, [open]);

  useEffect(() => {
    if (open) search.current?.focus();
  }, [open]);

  useEffect(() => {
    setActive(flat.length > 0 ? flat[0].chain_id : null);
  }, [flat]);

  useEffect(() => {
    if (!open || active === null) return;
    list.current
      ?.querySelector<HTMLElement>(`[data-chain="${active}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }, [active, open]);

  const toggle = () => {
    if (!selectable) return;
    if (!open) {
      onOpen();
      setQuery('');
    }
    setOpen(!open);
  };

  const pick = (chainId: number) => {
    setOpen(false);
    if (chainId !== chain?.chain_id) onChange(chainId);
  };

  const changeVisible = (next: Set<string>) => {
    const all = next.size === 0 || next.size === known.size;
    const stored = all ? new Set<string>() : next;
    setVisible(stored);
    writeEcosystems(stored);
  };

  const toggleEcosystem = (key: string) => {
    const next = new Set(shown);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    changeVisible(next);
  };

  const changeSort = (next: SortKey) => {
    setSort(next);
    writeSort(next);
  };

  const move = (step: number) => {
    if (flat.length === 0) return;
    const index = flat.findIndex((candidate) => candidate.chain_id === active);
    const next = (index + step + flat.length) % flat.length;
    setActive(flat[next].chain_id);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      move(1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      move(-1);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (active !== null) pick(active);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      setOpen(false);
    }
  };

  return (
    <div ref={root} className="relative flex h-9 flex-none items-center border-l border-hairline pl-3">
      <button
        type="button"
        className={cx(
          'flex h-8 items-center gap-2 rounded-ui-sm px-1.5 text-xs font-medium text-text-secondary [transition:background_120ms_ease]',
          selectable && 'cursor-pointer hover:bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)] hover:text-text-primary',
          open && 'bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)] text-text-primary',
          focusRing,
        )}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="Network"
        disabled={!selectable}
        title={chain ? `${chain.name} · chain ${chain.chain_id} · ${chain.rpc_nodes} rpc nodes` : 'Ethereum network'}
        onClick={toggle}
      >
        <span className={cx('grid size-5 place-content-center rounded-full text-[10px] font-semibold', toneGlyph[tone])}>
          {chainGlyph(chain)}
        </span>
        <span className="max-w-[16ch] overflow-hidden text-ellipsis whitespace-nowrap">{chain?.name ?? 'Ethereum'}</span>
        {chains.length > 1 && (
          <IconChevron className={cx('text-text-muted [transition:transform_120ms_ease]', open && 'rotate-180')} size={13} />
        )}
      </button>

      {open && (
        <Panel
          className="absolute left-3 top-[calc(100%+6px)] z-30 flex max-h-[min(520px,calc(100vh-90px))] w-[min(360px,calc(100vw-32px))] flex-col overflow-hidden bg-surface-1 shadow-pop"
          onKeyDown={onKeyDown}
        >
          <div className="flex flex-col gap-2 border-b border-hairline p-2.5">
            <label className="flex h-8 items-center gap-2 rounded-ui-sm border border-hairline bg-plane px-2.5 text-text-muted focus-within:border-accent focus-within:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_22%,transparent)]">
              <IconSearch size={14} />
              <input
                ref={search}
                className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-text-primary outline-none placeholder:text-text-muted"
                placeholder={`Search ${chains.length} networks`}
                value={query}
                spellCheck={false}
                autoComplete="off"
                aria-controls="chain-picker-list"
                aria-activedescendant={active === null ? undefined : `chain-option-${active}`}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            {ecosystems.length > 1 && (
              <div className="flex flex-wrap gap-1" role="group" aria-label="Chain ecosystems">
                <button
                  type="button"
                  aria-pressed={shown.size === 0}
                  className={chip(shown.size === 0)}
                  onClick={() => changeVisible(new Set())}
                >
                  All
                  <span className="tabular-nums opacity-70">{[...hits.values()].reduce((sum, count) => sum + count, 0)}</span>
                </button>
                {ecosystems.map((ecosystem) => {
                  const on = shown.has(ecosystem.key);
                  return (
                    <button
                      key={ecosystem.key}
                      type="button"
                      aria-pressed={on}
                      className={chip(on)}
                      title={on ? `Hide ${ecosystem.title}` : `Show ${ecosystem.title}`}
                      onClick={() => toggleEcosystem(ecosystem.key)}
                    >
                      <span className={cx('size-1.5 rounded-full', toneDot[ecosystem.tone])} />
                      {ecosystem.title}
                      <span className="tabular-nums opacity-70">{hits.get(ecosystem.key) ?? 0}</span>
                    </button>
                  );
                })}
              </div>
            )}
            <div className="flex items-center gap-2 text-[11px] text-text-muted">
              <span>Sort</span>
              <div className="flex rounded-ui-sm border border-hairline bg-plane p-0.5" role="radiogroup" aria-label="Sort networks">
                {SORTS.map((option) => (
                  <button
                    key={option.key}
                    type="button"
                    role="radio"
                    aria-checked={sort === option.key}
                    className={cx(
                      'h-6 cursor-pointer rounded-[4px] px-2 font-medium [transition:background_120ms_ease,color_120ms_ease]',
                      sort === option.key
                        ? 'bg-surface-2 text-text-primary shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
                        : 'text-text-muted hover:text-text-primary',
                      focusRing,
                    )}
                    onClick={() => changeSort(option.key)}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
              <span className="ml-auto tabular-nums">{flat.length} shown</span>
            </div>
          </div>

          <div ref={list} id="chain-picker-list" role="listbox" aria-label="Networks" className="min-h-0 flex-1 overflow-y-auto pb-1.5">
            {groups.length === 0 && (
              <div className="px-4 py-8 text-center text-xs text-text-muted">{needle ? <>No network matches “{query.trim()}”</> : 'No networks in the picked ecosystems'}</div>
            )}
            {groups.map((group) => (
              <div key={group.key} role="group" aria-label={group.title}>
                <div className="sticky top-0 z-10 flex items-center gap-2 border-b border-hairline bg-surface-1 px-3 pb-1.5 pt-2.5 text-[10px] font-semibold uppercase tracking-[0.07em] text-text-muted">
                  <span className={cx('size-1.5 rounded-full', toneDot[group.tone])} />
                  <span>{group.title}</span>
                  <span className="ml-auto font-medium tabular-nums">{group.chains.length}</span>
                </div>
                <div className="px-1.5 pt-1">
                  {group.chains.map((option) => {
                    const selected = option.chain_id === chain?.chain_id;
                    const highlighted = option.chain_id === active;
                    return (
                      <div
                        key={option.chain_id}
                        id={`chain-option-${option.chain_id}`}
                        data-chain={option.chain_id}
                        role="option"
                        aria-selected={selected}
                        className={cx(
                          'flex h-9 cursor-pointer items-center gap-2.5 rounded-ui-sm px-2 text-[13px]',
                          highlighted && 'bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)]',
                          selected && 'text-accent',
                          option.rpc_nodes === 0 && !selected && 'text-text-muted',
                        )}
                        onPointerMove={() => setActive(option.chain_id)}
                        onClick={() => pick(option.chain_id)}
                      >
                        <span className={cx('grid size-5 flex-none place-content-center rounded-full text-[10px] font-semibold', toneGlyph[toneOf(option)])}>
                          {chainGlyph(option)}
                        </span>
                        <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                          {option.name}
                          {option.chain_id === defaultChain && (
                            <span className="ml-1.5 text-[10px] uppercase tracking-[0.06em] text-text-muted">default</span>
                          )}
                        </span>
                        <span className="font-mono-ui text-[11px] tabular-nums text-text-muted">{option.chain_id}</span>
                        <span
                          className={cx('size-1.5 flex-none rounded-full', option.rpc_nodes > 0 ? 'bg-good' : 'bg-text-muted opacity-50')}
                          title={option.rpc_nodes > 0 ? `${option.rpc_nodes} rpc nodes` : 'no rpc yet'}
                        />
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        </Panel>
      )}
    </div>
  );
}
