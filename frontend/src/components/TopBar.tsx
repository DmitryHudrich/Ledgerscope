import { useState, type FormEvent, type ReactNode } from 'react';

import type { ChainInfo, GraphRoot, GraphScope } from '../api/types';
import { addressHint, displayAddress, parseAddressInput } from '../lib/chain';
import type { Theme } from '../lib/theme';
import type { BlockBounds } from './Timeline';
import { IconChevron, IconLogo, IconMoon, IconSun } from './Icons';
import { ChainPicker } from './ChainPicker';
import { BlockSpanText } from './BlockSpanText';
import { Button, Input, cx } from './ui';

interface Props {
  chains: ChainInfo[];
  chain: ChainInfo | null;
  defaultChain: number | null;
  onChainChange: (chainId: number) => void;
  onChainsRefresh: () => void;
  roots: GraphRoot[];
  maxRoots: number;
  scope: GraphScope;
  onScopeChange: (next: GraphScope) => void;
  selection: BlockBounds;
  onSubmit: (address: string | null) => void;
  loading: boolean;
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
  scope,
  onScopeChange,
  selection,
  onSubmit,
  loading,
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
  const sweeping = scope === 'blocks';
  const canBuild = sweeping || roots.length > 0 || (parsed !== null && !full);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (sweeping) {
      if (!loading) onSubmit(null);
      return;
    }
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
        <div className="flex h-9 flex-none items-center rounded-ui-sm border border-hairline p-0.5" role="radiogroup" aria-label="Graph scope">
          {(['roots', 'blocks'] as const).map((value) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={scope === value}
              title={value === 'roots' ? 'Walk out from the addresses you add' : 'Every transaction inside the block range'}
              className={cx(
                'h-full cursor-pointer rounded-[5px] border-0 bg-transparent px-2.5 text-xs font-medium text-text-muted',
                scope === value && 'bg-[color-mix(in_srgb,var(--accent)_14%,transparent)] text-accent',
              )}
              onClick={() => onScopeChange(value)}
            >
              {value === 'roots' ? 'Addresses' : 'All txs'}
            </button>
          ))}
        </div>
        <label className="sr-only" htmlFor="q-wallet">Wallet or address</label>
        <Input
          id="q-wallet"
          className="h-9 min-w-[18ch] flex-1 font-mono-ui text-[13px]"
          invalid={!sweeping && invalid}
          value={sweeping ? '' : draft}
          disabled={sweeping}
          spellCheck={false}
          autoComplete="off"
          placeholder={sweeping ? 'Every transaction in the block range' : `Search or paste ${chain?.name ?? 'Ethereum'} ${addressHint()}`}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button className="h-9 px-4" variant="primary" type="submit" disabled={!canBuild || (!sweeping && (invalid || full)) || loading}>
          {loading ? 'Building…' : 'Build graph'}
        </Button>
        <Button
          variant="ghost"
          className={cx('h-9 flex-none gap-1.5 px-2.5 text-xs font-medium text-text-muted', advancedOpen && 'text-text-primary')}
          onClick={() => onAdvancedOpenChange(!advancedOpen)}
          aria-pressed={advancedOpen}
          title="Block range & scan ranges"
        >
          <BlockSpanText from={selection.from} to={selection.to} />
          <IconChevron className={cx('transition-transform', advancedOpen && 'rotate-180')} />
        </Button>
      </form>

      <div className="flex-1" />

      <div className="ml-auto flex min-w-0 items-center gap-1.5">
        {canvasSlot}
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
