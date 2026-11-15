import { useEffect, useMemo, useState } from 'react';

import type { ChainInfo } from '../../api/types';
import { canvasApi, ownerName, type CanvasInfo, type Me } from '../../api/account';
import { formatRelative } from '../../lib/format';
import { FormError, Modal, errorText } from '../Modal';
import { IconSearch } from '../Icons';
import { Badge, Button, PanelTitle, cx, focusRing } from '../ui';

interface Props {
  me: Me;
  chains: ChainInfo[];
  currentId: string | null;
  onOpen: (canvas: CanvasInfo) => void;
  onShare: (canvas: CanvasInfo) => void;
  onDeleted: (id: string) => void;
  onSaveNew: () => void;
  onDetach?: () => void;
  onClose: () => void;
}

interface Group {
  key: string;
  title: string;
  canvases: CanvasInfo[];
}

const ACCESS_TITLE = { view: 'View only', edit: 'Can edit', manage: 'Owner' } as const;

function groupsOf(canvases: CanvasInfo[], me: Me): Group[] {
  const personal: CanvasInfo[] = [];
  const shared: CanvasInfo[] = [];
  const teams = new Map<string, Group>();

  for (const canvas of canvases) {
    if (canvas.owner.kind === 'user' && canvas.owner.id === me.user.id) personal.push(canvas);
    else if (canvas.owner.kind === 'team' && me.teams.some((team) => team.id === canvas.owner.id)) {
      const group = teams.get(canvas.owner.id) ?? { key: canvas.owner.id, title: ownerName(canvas.owner), canvases: [] };
      group.canvases.push(canvas);
      teams.set(canvas.owner.id, group);
    } else shared.push(canvas);
  }

  return [
    { key: 'personal', title: 'Personal', canvases: personal },
    ...[...teams.values()].sort((a, b) => a.title.localeCompare(b.title)),
    { key: 'shared', title: 'Shared with you', canvases: shared },
  ].filter((group) => group.canvases.length > 0);
}

export function CanvasesDialog({ me, chains, currentId, onOpen, onShare, onDeleted, onSaveNew, onDetach, onClose }: Props) {
  const [canvases, setCanvases] = useState<CanvasInfo[] | null>(null);
  const [query, setQuery] = useState('');
  const [renaming, setRenaming] = useState<string | null>(null);
  const [title, setTitle] = useState('');
  const [error, setError] = useState<string | null>(null);

  const reload = async () => {
    try {
      setCanvases(await canvasApi.list());
    } catch (cause) {
      setError(errorText(cause));
      setCanvases([]);
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  const chainName = (chainId: number) => chains.find((chain) => chain.chain_id === chainId)?.name ?? `Chain ${chainId}`;
  const needle = query.trim().toLowerCase();
  const groups = useMemo(
    () => groupsOf((canvases ?? []).filter((canvas) => !needle || canvas.title.toLowerCase().includes(needle)), me),
    [canvases, me, needle],
  );

  const rename = async (canvas: CanvasInfo) => {
    try {
      await canvasApi.update(canvas.id, canvas.version, { title });
      setRenaming(null);
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  const remove = async (canvas: CanvasInfo) => {
    if (!window.confirm(`Delete “${canvas.title}” for everyone who can see it?`)) return;
    try {
      await canvasApi.delete(canvas.id);
      onDeleted(canvas.id);
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  return (
    <Modal
      title="Canvases"
      subtitle="Your saved investigations, your teams’ and the ones shared with you."
      onClose={onClose}
      width="lg"
      footer={(
        <>
          {onDetach && <Button className="mr-auto" onClick={onDetach} title="Keep the graph, stop saving into the open canvas">Start a new canvas</Button>}
          <Button variant="primary" onClick={onSaveNew}>Save current as new…</Button>
        </>
      )}
    >
      <div className="grid gap-3">
        <label className="flex h-8 items-center gap-2 rounded-ui-sm border border-hairline bg-plane px-2.5 text-text-muted focus-within:border-accent">
          <IconSearch size={14} />
          <input
            className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-text-primary outline-none placeholder:text-text-muted"
            placeholder="Search by title"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <FormError message={error} />
        {canvases === null && <p className="m-0 text-[13px] text-text-muted">Loading…</p>}
        {canvases !== null && groups.length === 0 && (
          <p className="m-0 py-6 text-center text-[13px] text-text-muted">
            {needle ? 'Nothing matches.' : 'No canvases yet. Build a graph and save it.'}
          </p>
        )}
        {groups.map((group) => (
          <section key={group.key} className="grid gap-1">
            <PanelTitle className="mb-1">{group.title} · {group.canvases.length}</PanelTitle>
            {group.canvases.map((canvas) => (
              <div
                key={canvas.id}
                className={cx(
                  'group flex items-center gap-3 rounded-ui-sm border px-3 py-2',
                  canvas.id === currentId ? 'border-[color-mix(in_srgb,var(--accent)_45%,transparent)] bg-[color-mix(in_srgb,var(--accent)_8%,transparent)]' : 'border-transparent hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]',
                )}
              >
                <div className="min-w-0 flex-1">
                  {renaming === canvas.id ? (
                    <form
                      className="flex gap-2"
                      onSubmit={(event) => {
                        event.preventDefault();
                        void rename(canvas);
                      }}
                    >
                      <input
                        className={cx('h-7 min-w-0 flex-1 rounded-ui-sm border border-hairline bg-plane px-2 text-[13px]', focusRing)}
                        maxLength={120}
                        value={title}
                        autoFocus
                        onChange={(event) => setTitle(event.target.value)}
                      />
                      <Button type="submit" className="h-7">Save</Button>
                      <Button className="h-7" onClick={() => setRenaming(null)}>Cancel</Button>
                    </form>
                  ) : (
                    <button
                      type="button"
                      className={cx('block w-full cursor-pointer overflow-hidden text-ellipsis whitespace-nowrap text-left text-[13px] font-medium', focusRing)}
                      onClick={() => onOpen(canvas)}
                    >
                      {canvas.title}
                    </button>
                  )}
                  <div className="mt-0.5 flex flex-wrap gap-x-2 text-[11px] text-text-muted">
                    <span>{chainName(canvas.chain_id)}</span>
                    <span>· updated {formatRelative(Date.parse(canvas.updated_at) / 1000)}</span>
                    <span>· v{canvas.version}</span>
                  </div>
                </div>
                <Badge>{ACCESS_TITLE[canvas.access]}</Badge>
                <div className="flex gap-1 opacity-70 group-hover:opacity-100">
                  <Button className="h-7 px-2 text-xs" onClick={() => onOpen(canvas)}>Open</Button>
                  {canvas.access !== 'view' && (
                    <Button variant="ghost" className="h-7 px-2 text-xs" onClick={() => { setRenaming(canvas.id); setTitle(canvas.title); }}>Rename</Button>
                  )}
                  {canvas.access === 'manage' && (
                    <>
                      <Button variant="ghost" className="h-7 px-2 text-xs" onClick={() => onShare(canvas)}>Share</Button>
                      <Button variant="ghost" className="h-7 px-2 text-xs text-critical" onClick={() => void remove(canvas)}>Delete</Button>
                    </>
                  )}
                </div>
              </div>
            ))}
          </section>
        ))}
      </div>
    </Modal>
  );
}
