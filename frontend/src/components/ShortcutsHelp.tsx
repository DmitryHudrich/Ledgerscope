import { SELECTION_SHORTCUTS } from '../lib/selection';
import { IconClose } from './Icons';
import { Button, Kbd, Panel } from './ui';

export function ShortcutsHelp({ onClose }: { onClose: () => void }) {
  return (
    <Panel
      className="absolute right-[calc(var(--inspector-w)+16px)] top-16 z-30 w-[340px] p-3 shadow-pop"
      role="dialog"
      aria-label="Selection shortcuts"
    >
      <div className="mb-2 flex items-center justify-between">
        <h2 className="m-0 text-[11px] font-semibold uppercase tracking-[0.08em] text-text-secondary">Selection shortcuts</h2>
        <Button variant="ghost" icon className="size-7" onClick={onClose} aria-label="Close shortcuts">
          <IconClose size={14} />
        </Button>
      </div>
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-xs">
        {SELECTION_SHORTCUTS.map((shortcut) => (
          <div key={shortcut.keys} className="contents">
            <dt className="whitespace-nowrap"><Kbd>{shortcut.keys}</Kbd></dt>
            <dd className="m-0 text-text-secondary">{shortcut.action}</dd>
          </div>
        ))}
      </dl>
    </Panel>
  );
}
