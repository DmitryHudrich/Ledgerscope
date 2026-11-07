import type { CanvasInfo } from '../../api/account';
import { Button, cx } from '../ui';

export type SaveState = 'clean' | 'dirty' | 'saving' | 'error';

interface Props {
  canvas: CanvasInfo | null;
  saveState: SaveState;
  publicView: boolean;
  onSave: () => void;
  onShare: () => void;
  onBrowse: () => void;
}

const STATE_TEXT: Record<SaveState, string> = {
  clean: 'Saved',
  dirty: 'Unsaved changes',
  saving: 'Saving…',
  error: 'Not saved',
};

export function CanvasControls({ canvas, saveState, publicView, onSave, onShare, onBrowse }: Props) {
  if (publicView) {
    return (
      <span className="rounded-full border border-hairline px-2.5 py-1 text-[11px] text-text-secondary" title="Opened from a public link">
        Read-only shared canvas
      </span>
    );
  }

  const editable = !canvas || canvas.access !== 'view';

  return (
    <div className="flex min-w-0 items-center gap-1.5">
      <button
        type="button"
        className="flex min-w-0 max-w-[22ch] cursor-pointer items-center gap-1.5 rounded-ui-sm px-1.5 py-1 text-xs text-text-secondary hover:bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)] hover:text-text-primary"
        title={canvas ? `${canvas.title} · v${canvas.version} · ${STATE_TEXT[saveState]}` : 'Open a saved canvas'}
        onClick={onBrowse}
      >
        <span
          className={cx(
            'size-1.5 flex-none rounded-full',
            !canvas ? 'bg-text-muted opacity-50' : saveState === 'clean' ? 'bg-good' : saveState === 'error' ? 'bg-critical' : 'bg-series-focus',
          )}
          aria-hidden="true"
        />
        <span className="truncate">{canvas ? canvas.title : 'Unsaved canvas'}</span>
      </button>
      {editable && (
        <Button className="h-8 px-2.5 text-xs" disabled={saveState === 'saving' || (canvas !== null && saveState === 'clean')} onClick={onSave} title="Save (Ctrl+S)">
          {canvas ? 'Save' : 'Save…'}
        </Button>
      )}
      {canvas?.access === 'view' && <span className="text-[11px] text-text-muted">View only</span>}
      {canvas?.access === 'manage' && (
        <Button variant="ghost" className="h-8 px-2.5 text-xs" onClick={onShare}>Share</Button>
      )}
    </div>
  );
}
