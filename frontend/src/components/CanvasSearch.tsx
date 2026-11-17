import { useEffect, useRef, useState } from 'react';

import { getNodeDisplayName } from '../graph/labels';
import { displayAddress } from '../lib/chain';
import { formatCount, shortAddress } from '../lib/format';
import { MIN_QUERY, type NodeHit } from '../lib/nodeSearch';
import { IconClose, IconSearch } from './Icons';
import { CopyAddress } from './CopyAddress';
import { Button, Dot, Kbd, Panel, cx } from './ui';

const SHOWN = 30;

interface Props {
  query: string;
  hits: NodeHit[];
  onQueryChange: (query: string) => void;
  onPick: (id: string) => void;
  onClose: () => void;
}

export function CanvasSearch({ query, hits, onQueryChange, onPick, onClose }: Props) {
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const shown = hits.slice(0, SHOWN);

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);

  useEffect(() => setActive(0), [query]);

  useEffect(() => {
    list.current?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [active]);

  const pick = (index: number) => {
    const hit = shown[index];
    if (hit) onPick(hit.node.id);
  };

  return (
    <Panel
      className="absolute right-[calc(var(--inspector-w)+72px)] top-4 z-20 grid w-[min(380px,calc(100vw-160px))] gap-1 p-1.5 shadow-pop"
      role="search"
      aria-label="Find wallets on the canvas"
    >
      <div className="flex items-center gap-1.5 px-1">
        <IconSearch size={14} className="flex-none text-text-muted" />
        <input
          ref={input}
          className="h-8 min-w-0 flex-1 bg-transparent font-mono-ui text-xs text-text-primary outline-none placeholder:font-sans placeholder:text-text-muted"
          value={query}
          spellCheck={false}
          autoComplete="off"
          placeholder="Address, part of it, label or tag"
          aria-label="Find wallets on the canvas"
          aria-controls="canvas-search-results"
          aria-activedescendant={shown[active] ? `canvas-hit-${active}` : undefined}
          onChange={(event) => onQueryChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown') {
              event.preventDefault();
              setActive((index) => Math.min(index + 1, shown.length - 1));
            } else if (event.key === 'ArrowUp') {
              event.preventDefault();
              setActive((index) => Math.max(index - 1, 0));
            } else if (event.key === 'Enter') {
              event.preventDefault();
              pick(active);
            } else if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              onClose();
            }
          }}
        />
        <span className="flex-none text-[11px] tabular-nums text-text-muted">
          {query.trim().length >= MIN_QUERY ? formatCount(hits.length) : ''}
        </span>
        <Button variant="ghost" icon className="size-7" onClick={onClose} aria-label="Close search">
          <IconClose size={13} />
        </Button>
      </div>

      {query.trim().length >= MIN_QUERY && (
        hits.length === 0 ? (
          <p className="m-0 px-2 pb-1.5 text-xs text-text-muted">No wallet on the canvas matches.</p>
        ) : (
          <ul id="canvas-search-results" ref={list} className="m-0 grid max-h-[320px] list-none gap-px overflow-y-auto overscroll-contain p-0" role="listbox">
            {shown.map((hit, index) => {
              const name = getNodeDisplayName(hit.node);
              const address = displayAddress(hit.node.id);
              return (
                <li
                  key={hit.node.id}
                  id={`canvas-hit-${index}`}
                  role="option"
                  aria-selected={index === active}
                  data-active={index === active}
                  className={cx(
                    'flex cursor-pointer items-center gap-2 rounded-ui-sm px-2 py-1.5',
                    index === active ? 'bg-[color-mix(in_srgb,var(--accent)_12%,transparent)]' : 'hover:bg-[color-mix(in_srgb,var(--text-primary)_5%,transparent)]',
                  )}
                  onMouseEnter={() => setActive(index)}
                  onClick={() => pick(index)}
                >
                  <Dot tone={hit.node.kind} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-xs text-text-primary">{name}</div>
                    <div className="truncate font-mono-ui text-[10px] text-text-muted" title={address}>
                      {name === shortAddress(hit.node.id, 6, 4) ? address : shortAddress(address, 10, 8)}
                      {hit.reason !== 'address' && <span className="font-sans"> · {hit.reason}</span>}
                    </div>
                  </div>
                  <CopyAddress address={hit.node.id} />
                  {hit.node.root && <span className="flex-none text-[10px] uppercase tracking-wide text-text-muted">root</span>}
                  <span className="flex-none text-[10px] tabular-nums text-text-muted" title="Transactions on the canvas">
                    {formatCount(hit.node.inCount + hit.node.outCount)} tx
                  </span>
                </li>
              );
            })}
            {hits.length > SHOWN && (
              <li className="px-2 py-1 text-[11px] text-text-muted">and {formatCount(hits.length - SHOWN)} more, all highlighted on the canvas</li>
            )}
          </ul>
        )
      )}

      <div className="flex gap-3 px-2 pb-0.5 text-[10px] text-text-muted">
        <span><Kbd>↑</Kbd><Kbd>↓</Kbd> pick</span>
        <span><Kbd>Enter</Kbd> center and select</span>
        <span><Kbd>Esc</Kbd> close</span>
      </div>
    </Panel>
  );
}
