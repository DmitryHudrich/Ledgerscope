import { useState, type FormEvent } from 'react';

import type { GraphRoot } from '../api/types';
import type { CoverageResponse, HistogramResponse } from '../api/types';
import type { Theme } from '../lib/theme';
import type { BlockBounds } from './Timeline';
import { IconChevron, IconLogo, IconMoon, IconSun } from './Icons';
import { BlockRangeControl } from './BlockRangeControl';
import { Button, Dot, Input } from './ui';

export const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

interface Props {
  roots: GraphRoot[];
  maxRoots: number;
  selection: BlockBounds;
  coverage: CoverageResponse | null;
  histogram: HistogramResponse | null;
  onSelectionChange: (next: BlockBounds) => void;
  onSubmit: (address: string | null) => void;
  loading: boolean;
  demo: boolean;
  onDemoChange: (next: boolean) => void;
  advancedOpen: boolean;
  onAdvancedOpenChange: (next: boolean) => void;
  theme: Theme;
  onThemeChange: (next: Theme) => void;
}

export function TopBar({
  roots,
  maxRoots,
  selection,
  coverage,
  histogram,
  onSelectionChange,
  onSubmit,
  loading,
  demo,
  onDemoChange,
  advancedOpen,
  onAdvancedOpenChange,
  theme,
  onThemeChange,
}: Props) {
  const [draft, setDraft] = useState(roots[0]?.address ?? '');

  const typed = draft.trim().toLowerCase();
  const invalid = typed.length > 0 && !ADDRESS_RE.test(typed);
  const known = roots.some((root) => root.address === typed);
  const full = roots.length >= maxRoots && !known;
  const canBuild = roots.length > 0 || (typed.length > 0 && !invalid && !full);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!loading && canBuild && !invalid) onSubmit(typed || null);
  };

  const updateBlock = (key: keyof BlockBounds, raw: string) => {
    const value = Number.parseInt(raw, 10);
    if (!Number.isFinite(value) || value < 0) return;
    const next = { ...selection, [key]: value };
    if (next.from <= next.to) onSelectionChange(next);
  };

  return (
    <header className="relative z-20 flex min-h-[58px] items-center gap-3 border-b border-hairline bg-surface-1 px-4 py-2 max-[900px]:flex-wrap max-[900px]:gap-x-3 max-[900px]:px-3">
      <div className="flex items-center gap-[10px] whitespace-nowrap font-[650] tracking-[-0.01em]">
        <IconLogo className="block text-accent" />
        <span>Ledgerscope</span>
      </div>

      <div
        className="flex h-9 flex-none items-center gap-2 border-l border-hairline pl-3 text-xs font-medium text-text-secondary"
        aria-label="Network: Ethereum"
        title="Ethereum network"
      >
        <span className="grid size-5 place-content-center rounded-full bg-[color-mix(in_srgb,var(--series-eoa)_16%,transparent)] text-[10px] font-semibold text-series-eoa">
          Ξ
        </span>
        <span>Ethereum</span>
        <IconChevron className="text-text-muted" size={13} />
      </div>

      <form
        className="flex min-w-0 max-w-[760px] flex-[1_1_600px] items-center gap-2 max-[900px]:order-3 max-[900px]:basis-full max-[900px]:max-w-none"
        onSubmit={submit}
      >
        <label className="sr-only" htmlFor="q-wallet">Wallet or address</label>
        <Input
          id="q-wallet"
          className="h-9 min-w-[18ch] flex-1 font-mono-ui text-[13px]"
          invalid={invalid}
          value={draft}
          spellCheck={false}
          autoComplete="off"
          placeholder="Search or paste Ethereum address"
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button className="h-9 px-4" variant="primary" type="submit" disabled={!canBuild || invalid || full || loading}>
          {loading ? 'Building…' : 'Build graph'}
        </Button>
        <BlockRangeControl
          from={selection.from}
          to={selection.to}
          coverage={coverage}
          histogram={histogram}
          open={advancedOpen}
          onOpenChange={onAdvancedOpenChange}
          onFromChange={(value) => updateBlock('from', value)}
          onToChange={(value) => updateBlock('to', value)}
          onRangeChange={onSelectionChange}
        />
      </form>

      <div className="flex-1" />

      <div className="ml-auto flex items-center gap-1.5">
        <Button
          aria-pressed={demo}
          onClick={() => onDemoChange(!demo)}
          title="Render generated sample data instead of calling the backend"
        >
          <Dot tone={demo ? 'focus' : 'eoa'} />
          {demo ? 'Demo mode' : 'Live data'}
        </Button>

        <Button
          variant="ghost"
          icon
          onClick={() => onThemeChange(theme === 'dark' ? 'light' : 'dark')}
          title={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
          aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
        >
          {theme === 'dark' ? <IconSun /> : <IconMoon />}
        </Button>
      </div>
    </header>
  );
}
