import { useState, type FormEvent, type ReactNode } from 'react';

import type { ChainInfo, GraphRoot } from '../api/types';
import type { CoverageResponse, HistogramResponse } from '../api/types';
import { addressHint, displayAddress, parseAddressInput } from '../lib/chain';
import type { Theme } from '../lib/theme';
import type { BlockBounds } from './Timeline';
import { IconLogo, IconMoon, IconSun } from './Icons';
import { ChainPicker } from './ChainPicker';
import { BlockRangeControl } from './BlockRangeControl';
import { Button, Dot, Input } from './ui';

interface Props {
  chains: ChainInfo[];
  chain: ChainInfo | null;
  defaultChain: number | null;
  onChainChange: (chainId: number) => void;
  onChainsRefresh: () => void;
  roots: GraphRoot[];
  maxRoots: number;
  maxBlocks: number;
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
  canvasSlot?: ReactNode;
  accountSlot?: ReactNode;
}

export function TopBar({
  chains,
  chain,
  defaultChain,
  onChainChange,
  onChainsRefresh,
  roots,
  maxRoots,
  maxBlocks,
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
  canvasSlot,
  accountSlot,
}: Props) {
  const [draft, setDraft] = useState(roots[0] ? displayAddress(roots[0].address) : '');

  const raw = draft.trim();
  const parsed = raw.length > 0 ? parseAddressInput(raw) : null;
  const typed = parsed ?? '';
  const invalid = raw.length > 0 && parsed === null;
  const known = roots.some((root) => root.address === typed);
  const full = roots.length >= maxRoots && !known;
  const canBuild = roots.length > 0 || (parsed !== null && !full);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!loading && canBuild && !invalid) onSubmit(typed || null);
  };

  return (
    <header className="relative z-20 flex min-h-[58px] items-center gap-3 border-b border-hairline bg-surface-1 px-4 py-2 max-[1100px]:flex-wrap max-[1100px]:gap-x-3 max-[1100px]:px-3">
      <div className="flex items-center gap-[10px] whitespace-nowrap font-[650] tracking-[-0.01em]">
        <IconLogo className="block text-accent" />
        <span>Ledgerscope</span>
      </div>

      <ChainPicker
        chains={chains}
        chain={chain}
        defaultChain={defaultChain}
        disabled={loading}
        onChange={onChainChange}
        onOpen={onChainsRefresh}
      />

      <form
        className="flex min-w-0 max-w-[760px] flex-[1_1_600px] items-center gap-2 max-[1100px]:order-3 max-[1100px]:basis-full max-[1100px]:max-w-none"
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
          placeholder={`Search or paste ${chain?.name ?? 'Ethereum'} ${addressHint()}`}
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
          limits={{ head: coverage?.chain_head ?? null, maxBlocks }}
          open={advancedOpen}
          onOpenChange={onAdvancedOpenChange}
          onRangeChange={onSelectionChange}
        />
      </form>

      <div className="flex-1" />

      <div className="ml-auto flex min-w-0 items-center gap-1.5">
        {canvasSlot}
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
        {accountSlot}
      </div>
    </header>
  );
}
