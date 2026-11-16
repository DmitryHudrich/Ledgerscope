import { useEffect, useMemo, useState } from 'react';

import type { ChainInfo } from '../../api/types';
import { canvasApi, ownerName, type CanvasInfo, type CanvasOpening, type CanvasVersion, type Me } from '../../api/account';
import { formatRelative } from '../../lib/format';
import { FormError, Modal, errorText } from '../Modal';
import { IconChevron, IconClose, IconSearch } from '../Icons';
import { Badge, Button, PanelTitle, cx, focusRing } from '../ui';

interface Props {
  me: Me;
  chains: ChainInfo[];
  currentId: string | null;
  onOpen: (canvas: CanvasInfo, at?: string) => void;
  onChanged: (canvas: CanvasInfo) => void;
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

export function CanvasesDialog({ me, chains, currentId, onOpen, onChanged, onShare, onDeleted, onSaveNew, onDetach, onClose }: Props) {
  const [expanded, setExpanded] = useState<string | null>(null);
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
      onChanged(await canvasApi.update(canvas.id, canvas.version, { title }));
      setRenaming(null);
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  const changeOpening = async (canvas: CanvasInfo, opening: CanvasOpening) => {
    try {
      onChanged(await canvasApi.update(canvas.id, canvas.version, { opening }));
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
                  'rounded-ui-sm border',
                  canvas.id === currentId ? 'border-[color-mix(in_srgb,var(--accent)_45%,transparent)] bg-[color-mix(in_srgb,var(--accent)_8%,transparent)]' : 'border-transparent hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]',
                )}
              >
              <div className="group flex items-center gap-2 px-2 py-2">
                <button
                  type="button"
                  className={cx('grid size-6 flex-none cursor-pointer place-items-center rounded-ui-sm text-text-muted hover:text-text-primary', focusRing)}
                  aria-expanded={expanded === canvas.id}
                  aria-label={`${expanded === canvas.id ? 'Hide' : 'Show'} snapshots and autosaves of ${canvas.title}`}
                  onClick={() => setExpanded(expanded === canvas.id ? null : canvas.id)}
                >
                  <IconChevron size={14} className={cx('transition-transform', expanded === canvas.id ? 'rotate-0' : '-rotate-90')} />
                </button>
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
                      title={canvas.opening === 'latest' ? 'Opens at the newest autosave or snapshot' : 'Opens at the last saved snapshot'}
                      onClick={() => onOpen(canvas)}
                    >
                      {canvas.title}
                    </button>
                  )}
                  <div className="mt-0.5 flex flex-wrap gap-x-2 text-[11px] text-text-muted">
                    <span>{chainName(canvas.chain_id)}</span>
                    <span>· updated {formatRelative(Date.parse(canvas.updated_at) / 1000)}</span>
                    <span>· v{canvas.version}</span>
                    <span>· opens at {canvas.opening === 'latest' ? 'latest autosave' : 'last snapshot'}</span>
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
              {expanded === canvas.id && (
                <Versions
                  canvas={canvas}
                  onOpen={(versionId) => onOpen(canvas, versionId)}
                  onOpeningChange={(opening) => void changeOpening(canvas, opening)}
                  onError={setError}
                />
              )}
              </div>
            ))}
          </section>
        ))}
      </div>
    </Modal>
  );
}

const VERSION_PREVIEW = 8;

function Versions({ canvas, onOpen, onOpeningChange, onError }: {
  canvas: CanvasInfo;
  onOpen: (versionId: string) => void;
  onOpeningChange: (opening: CanvasOpening) => void;
  onError: (message: string) => void;
}) {
  const [versions, setVersions] = useState<CanvasVersion[] | null>(null);
  const [all, setAll] = useState(false);
  const editable = canvas.access !== 'view';

  useEffect(() => {
    let alive = true;
    canvasApi.versions(canvas.id)
      .then((loaded) => alive && setVersions(loaded))
      .catch((cause) => {
        if (!alive) return;
        setVersions([]);
        onError(errorText(cause));
      });
    return () => {
      alive = false;
    };
  }, [canvas.id, canvas.version, onError]);

  const remove = async (version: CanvasVersion) => {
    try {
      await canvasApi.deleteVersion(canvas.id, version.id);
      setVersions((current) => current?.filter((candidate) => candidate.id !== version.id) ?? null);
    } catch (cause) {
      onError(errorText(cause));
    }
  };

  const shown = all ? versions ?? [] : (versions ?? []).slice(0, VERSION_PREVIEW);

  return (
    <div className="grid gap-1 border-t border-hairline px-3 pb-2 pt-2">
      <div className="flex flex-wrap items-center gap-2 text-[11px] text-text-muted">
        <span>Clicking the canvas opens</span>
        <div className="flex rounded-ui-sm border border-hairline p-0.5" role="radiogroup" aria-label="Clicking the canvas opens">
          {(['latest', 'snapshot'] as const).map((opening) => (
            <button
              key={opening}
              type="button"
              role="radio"
              aria-checked={canvas.opening === opening}
              disabled={!editable}
              className={cx(
                'h-6 cursor-pointer rounded-[4px] border-0 bg-transparent px-2 text-[11px] disabled:cursor-default',
                canvas.opening === opening ? 'bg-[color-mix(in_srgb,var(--accent)_14%,transparent)] text-accent' : 'text-text-secondary',
              )}
              onClick={() => canvas.opening !== opening && onOpeningChange(opening)}
            >
              {opening === 'latest' ? 'the latest autosave' : 'the last snapshot'}
            </button>
          ))}
        </div>
      </div>
      {versions === null && <p className="m-0 text-[11px] text-text-muted">Loading versions…</p>}
      {versions !== null && versions.length === 0 && (
        <p className="m-0 text-[11px] text-text-muted">No snapshots yet. Save the canvas to make one.</p>
      )}
      <ul className="m-0 grid list-none gap-0.5 p-0">
        {shown.map((version) => (
          <li key={version.id} className="group/version flex items-center gap-2 rounded-ui-sm px-1.5 py-1 hover:bg-[color-mix(in_srgb,var(--text-primary)_5%,transparent)]">
            <span
              className={cx('flex-none rounded-full px-1.5 py-px text-[9px] font-semibold uppercase tracking-wide', version.kind === 'snapshot' ? 'bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] text-accent' : 'bg-[color-mix(in_srgb,var(--text-primary)_8%,transparent)] text-text-muted')}
            >
              {version.kind}
            </span>
            <button
              type="button"
              className={cx('min-w-0 flex-1 cursor-pointer truncate border-0 bg-transparent p-0 text-left text-xs text-text-primary hover:text-accent', focusRing)}
              title={`Open this ${version.kind}`}
              onClick={() => onOpen(version.id)}
            >
              {version.label || (version.kind === 'autosave' ? 'Autosave' : 'Snapshot')}
            </button>
            <span className="flex-none text-[11px] tabular-nums text-text-muted" title={new Date(version.created_at).toLocaleString()}>
              {formatRelative(Date.parse(version.created_at) / 1000)} · {version.author}
            </span>
            {editable && (
              <Button
                variant="ghost"
                icon
                className="size-6 opacity-0 group-hover/version:opacity-100 focus-visible:opacity-100"
                aria-label={`Delete this ${version.kind}`}
                onClick={() => void remove(version)}
              >
                <IconClose size={12} />
              </Button>
            )}
          </li>
        ))}
      </ul>
      {versions !== null && versions.length > VERSION_PREVIEW && (
        <Button variant="ghost" className="h-6 justify-self-start px-2 text-[11px]" onClick={() => setAll(!all)}>
          {all ? 'Show fewer' : `Show all ${versions.length}`}
        </Button>
      )}
    </div>
  );
}
