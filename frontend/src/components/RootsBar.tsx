import { useState, type FormEvent } from 'react';

import type { GraphRoot } from '../api/types';
import { shortAddress } from '../lib/format';
import { IconClose } from './Icons';

export const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

interface Props {
  roots: GraphRoot[];
  maxDepth: number;
  maxRoots: number;
  loading: boolean;
  onAdd: (address: string) => void;
  onRemove: (address: string) => void;
  onDepthChange: (address: string, depth: number) => void;
  onSubmit: () => void;
}

export function RootsBar({
  roots,
  maxDepth,
  maxRoots,
  loading,
  onAdd,
  onRemove,
  onDepthChange,
  onSubmit,
}: Props) {
  const [draft, setDraft] = useState('');

  const typed = draft.trim();
  const invalid = typed.length > 0 && !ADDRESS_RE.test(typed);
  const known = roots.some((root) => root.address === typed.toLowerCase());
  const full = roots.length >= maxRoots;

  const add = (event: FormEvent) => {
    event.preventDefault();
    if (invalid || typed.length === 0 || known || full) return;
    onAdd(typed.toLowerCase());
    setDraft('');
  };

  return (
    <div className="rootsbar">
      <form className="roots-add" onSubmit={add}>
        <input
          id="q-root"
          className={`input input-address${invalid ? ' input-invalid' : ''}`}
          value={draft}
          spellCheck={false}
          autoComplete="off"
          placeholder="0x… add an address to the canvas"
          aria-label="Address to investigate"
          aria-invalid={invalid}
          onChange={(event) => setDraft(event.target.value)}
        />
        <button
          type="submit"
          className="btn"
          disabled={invalid || typed.length === 0 || known || full}
          title={full ? `At most ${maxRoots} addresses at a time` : 'Add to the canvas'}
        >
          Add
        </button>
      </form>

      <div className="roots-list">
        {roots.length === 0 && <span className="roots-hint">No addresses on the canvas yet.</span>}

        {roots.map((root) => (
          <span className="root-chip" key={root.address}>
            <span className="root-address" title={root.address}>
              {shortAddress(root.address)}
            </span>

            <span className="root-depth">
              <button
                type="button"
                className="btn btn-ghost btn-step"
                aria-label={`Less depth for ${root.address}`}
                disabled={root.depth <= 0}
                onClick={() => onDepthChange(root.address, root.depth - 1)}
              >
                −
              </button>
              <span className="root-depth-value" title="Hops walked from this address">
                {root.depth}
              </span>
              <button
                type="button"
                className="btn btn-ghost btn-step"
                aria-label={`More depth for ${root.address}`}
                disabled={root.depth >= maxDepth}
                onClick={() => onDepthChange(root.address, root.depth + 1)}
              >
                +
              </button>
            </span>

            <button
              type="button"
              className="btn btn-ghost btn-icon btn-step"
              aria-label={`Remove ${root.address}`}
              onClick={() => onRemove(root.address)}
            >
              <IconClose />
            </button>
          </span>
        ))}
      </div>

      <button
        type="button"
        className="btn btn-primary"
        disabled={roots.length === 0 || loading}
        onClick={onSubmit}
      >
        {loading ? 'Loading…' : 'Build graph'}
      </button>
    </div>
  );
}
