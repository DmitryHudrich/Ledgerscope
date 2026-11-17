import type { CanvasInfo, CanvasVersion } from '../../api/account';
import { formatRelative } from '../../lib/format';
import { Button, cx } from '../ui';

export type SaveState = 'clean' | 'dirty' | 'saving' | 'error';

interface Props {
  canvas: CanvasInfo | null;
  saveState: SaveState;
  publicView: boolean;
  openedVersion: CanvasVersion | null;
  lastAutosave: string | null;
  autosaveFailed: boolean;
  onSave: () => void;
  onSnapshot: () => void;
  onShare: () => void;
  onBrowse: () => void;
}

const STATE_TEXT: Record<SaveState, string> = {
  clean: 'Saved',
  dirty: 'Changes since the last snapshot',
  saving: 'Saving…',
  error: 'Not saved',
};

function versionName(version: CanvasVersion): string {
  if (version.label) return `“${version.label}”`;
  return version.kind === 'autosave' ? 'an autosave' : 'a snapshot';
}

export function CanvasControls({
  canvas,
  saveState,
  publicView,
  openedVersion,
  lastAutosave,
  autosaveFailed,
  onSave,
  onSnapshot,
  onShare,
  onBrowse,
}: Props) {
  if (publicView) {
    return (
      <span className="rounded-full border border-hairline px-2.5 py-1 text-[11px] text-text-secondary" title="Opened from a public link">
        Read-only shared canvas
      </span>
    );
  }

  const editable = !canvas || canvas.access !== 'view';
  const status = !canvas
    ? null
    : autosaveFailed
      ? 'autosave failed'
      : saveState === 'saving'
        ? 'saving…'
        : lastAutosave && saveState !== 'clean'
          ? `autosaved ${formatRelative(Date.parse(lastAutosave) / 1000)}`
          : saveState === 'clean'
            ? 'saved'
            : null;
  const details = canvas
    ? [
        canvas.title,
        `v${canvas.version}`,
        STATE_TEXT[saveState],
        openedVersion ? `opened from ${versionName(openedVersion)}` : null,
      ].filter(Boolean).join(' · ')
    : 'Open a saved canvas';

  return (
    <div className="flex min-w-0 items-center gap-1.5">
      <button
        type="button"
        className="flex min-w-0 max-w-[30ch] cursor-pointer items-center gap-1.5 rounded-ui-sm px-1.5 py-1 text-xs text-text-secondary hover:bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)] hover:text-text-primary"
        title={details}
        onClick={onBrowse}
      >
        <span
          className={cx(
            'size-1.5 flex-none rounded-full',
            !canvas ? 'bg-text-muted opacity-50' : saveState === 'clean' ? 'bg-good' : saveState === 'error' || autosaveFailed ? 'bg-critical' : 'bg-series-focus',
          )}
          aria-hidden="true"
        />
        <span className="truncate">{canvas ? canvas.title : 'Unsaved canvas'}</span>
        {status && <span className={cx('flex-none text-[10px]', autosaveFailed ? 'text-critical' : 'text-text-muted')}>{status}</span>}
      </button>
      {editable && (
        <Button className="h-8 px-2.5 text-xs" disabled={saveState === 'saving' || (canvas !== null && saveState === 'clean')} onClick={onSave} title="Save a snapshot (Ctrl+S)">
          {canvas ? 'Save' : 'Save…'}
        </Button>
      )}
      {editable && canvas && (
        <Button variant="ghost" className="h-8 px-2 text-xs" disabled={saveState === 'saving'} onClick={onSnapshot} title="Save a named snapshot">
          Snapshot…
        </Button>
      )}
      {canvas?.access === 'view' && <span className="text-[11px] text-text-muted">View only</span>}
      {canvas?.access === 'manage' && (
        <Button variant="ghost" className="h-8 px-2.5 text-xs" onClick={onShare}>Share</Button>
      )}
    </div>
  );
}
