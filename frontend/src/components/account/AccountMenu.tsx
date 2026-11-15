import { useEffect, useRef, useState } from 'react';

import type { Me } from '../../api/account';
import { Button, Panel, cx, focusRing } from '../ui';

export type AccountAction = 'canvases' | 'teams' | 'labels' | 'rpc' | 'tls' | 'profile' | 'info' | 'logout';

interface Props {
  me: Me | null;
  personalRpc: boolean;
  onSignIn: () => void;
  onAction: (action: AccountAction) => void;
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const letters = parts.length > 1 ? parts[0][0] + parts[parts.length - 1][0] : name.slice(0, 2);
  return letters.toUpperCase();
}

const ITEMS: Array<{ action: AccountAction; label: string; hint?: string }> = [
  { action: 'canvases', label: 'Canvases', hint: 'Open, rename, share' },
  { action: 'teams', label: 'Teams', hint: 'Members and roles' },
  { action: 'labels', label: 'My labels', hint: 'Yours and your teams’' },
  { action: 'rpc', label: 'My RPC nodes', hint: 'Asked first on your requests' },
  { action: 'tls', label: 'TLS deanon', hint: 'IP to wallet from rpc traffic' },
  { action: 'profile', label: 'Profile', hint: 'Name, password, sessions' },
  { action: 'info', label: 'Info', hint: 'Stored transactions, rpc traffic' },
];

export function AccountMenu({ me, personalRpc, onSignIn, onAction }: Props) {
  const root = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  if (!me) {
    return <Button onClick={onSignIn}>Sign in</Button>;
  }

  const pick = (action: AccountAction) => {
    setOpen(false);
    onAction(action);
  };

  return (
    <div ref={root} className="relative">
      <button
        type="button"
        className={cx(
          'grid size-8 cursor-pointer place-content-center rounded-full bg-[color-mix(in_srgb,var(--accent)_18%,transparent)] text-[11px] font-semibold text-accent [transition:box-shadow_120ms_ease] hover:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_18%,transparent)]',
          focusRing,
        )}
        aria-haspopup="menu"
        aria-expanded={open}
        title={`${me.user.name} · ${me.user.email}`}
        onClick={() => setOpen(!open)}
      >
        {initials(me.user.name)}
      </button>
      {open && (
        <Panel className="absolute right-0 top-[calc(100%+6px)] z-40 w-[248px] overflow-hidden bg-surface-1 shadow-pop" role="menu">
          <div className="border-b border-hairline px-3 py-2.5">
            <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-semibold">{me.user.name}</div>
            <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-text-muted">{me.user.email}</div>
            {me.teams.length > 0 && (
              <div className="mt-1 text-[11px] text-text-muted">
                {me.teams.length === 1 ? me.teams[0].name : `${me.teams.length} teams`}
              </div>
            )}
          </div>
          <div className="p-1">
            {ITEMS.filter((item) => item.action !== 'rpc' || personalRpc).map((item) => (
              <button
                key={item.action}
                type="button"
                role="menuitem"
                className={cx('flex w-full cursor-pointer flex-col items-start rounded-ui-sm px-2.5 py-1.5 text-left hover:bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)]', focusRing)}
                onClick={() => pick(item.action)}
              >
                <span className="text-[13px]">{item.label}</span>
                {item.hint && <span className="text-[11px] text-text-muted">{item.hint}</span>}
              </button>
            ))}
          </div>
          <div className="border-t border-hairline p-1">
            <button
              type="button"
              role="menuitem"
              className={cx('w-full cursor-pointer rounded-ui-sm px-2.5 py-1.5 text-left text-[13px] text-critical hover:bg-[color-mix(in_srgb,var(--critical)_8%,transparent)]', focusRing)}
              onClick={() => pick('logout')}
            >
              Sign out
            </button>
          </div>
        </Panel>
      )}
    </div>
  );
}
