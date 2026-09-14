import type { FormEvent } from 'react';

import type { GraphQuery } from '../api/types';
import type { Theme } from '../lib/theme';
import { IconLogo, IconMoon, IconSun } from './Icons';

export const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

export interface QueryDraft {
  wallet: string;
  from: string;
  to: string;
}

export function parseDraft(draft: QueryDraft): GraphQuery | null {
  const from = Number.parseInt(draft.from, 10);
  const to = Number.parseInt(draft.to, 10);
  if (!ADDRESS_RE.test(draft.wallet.trim())) return null;
  if (!Number.isFinite(from) || !Number.isFinite(to) || from < 0 || to < from) return null;
  return { wallet: draft.wallet.trim().toLowerCase(), from, to };
}

interface Props {
  draft: QueryDraft;
  onDraftChange: (next: QueryDraft) => void;
  onSubmit: () => void;
  loading: boolean;
  demo: boolean;
  onDemoChange: (next: boolean) => void;
  theme: Theme;
  onThemeChange: (next: Theme) => void;
}

export function TopBar({
  draft,
  onDraftChange,
  onSubmit,
  loading,
  demo,
  onDemoChange,
  theme,
  onThemeChange,
}: Props) {
  const parsed = parseDraft(draft);
  const walletInvalid = draft.wallet.length > 0 && !ADDRESS_RE.test(draft.wallet.trim());
  const rangeInvalid =
    draft.from.length > 0 && draft.to.length > 0 && !parsed && !walletInvalid;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (parsed && !loading) onSubmit();
  };

  return (
    <header className="topbar">
      <div className="brand">
        <IconLogo className="brand-mark" />
        <span>Ledgerscope</span>
        <span className="brand-sub">ETH transaction graph</span>
      </div>

      <form className="query" onSubmit={submit}>
        <div className="field">
          <label htmlFor="q-wallet">Wallet</label>
          <input
            id="q-wallet"
            className={`input input-address${walletInvalid ? ' input-invalid' : ''}`}
            value={draft.wallet}
            spellCheck={false}
            autoComplete="off"
            placeholder="0x…"
            aria-invalid={walletInvalid}
            onChange={(e) => onDraftChange({ ...draft, wallet: e.target.value })}
          />
        </div>
        <div className="field">
          <label htmlFor="q-from">From block</label>
          <input
            id="q-from"
            className={`input input-block${rangeInvalid ? ' input-invalid' : ''}`}
            value={draft.from}
            inputMode="numeric"
            onChange={(e) => onDraftChange({ ...draft, from: e.target.value.replace(/\D/g, '') })}
          />
        </div>
        <div className="field">
          <label htmlFor="q-to">To block</label>
          <input
            id="q-to"
            className={`input input-block${rangeInvalid ? ' input-invalid' : ''}`}
            value={draft.to}
            inputMode="numeric"
            onChange={(e) => onDraftChange({ ...draft, to: e.target.value.replace(/\D/g, '') })}
          />
        </div>
        <div className="field">
          <label aria-hidden="true">&nbsp;</label>
          <button className="btn btn-primary" type="submit" disabled={!parsed || loading}>
            {loading ? 'Loading…' : 'Build graph'}
          </button>
        </div>
      </form>

      <div className="topbar-spacer" />

      <button
        type="button"
        className="btn"
        aria-pressed={demo}
        onClick={() => onDemoChange(!demo)}
        title="Render a generated sample instead of calling the backend"
      >
        Demo data
      </button>

      <button
        type="button"
        className="btn btn-ghost btn-icon"
        onClick={() => onThemeChange(theme === 'dark' ? 'light' : 'dark')}
        title={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
        aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
      >
        {theme === 'dark' ? <IconSun /> : <IconMoon />}
      </button>
    </header>
  );
}
