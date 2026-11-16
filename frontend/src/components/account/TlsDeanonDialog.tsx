import { useEffect, useState } from 'react';
import { BlockSpanText } from '../BlockSpanText';

import type { ChainInfo } from '../../api/types';
import { tlsApi, type CorrelateResult, type IpWalletLink } from '../../api/tls';
import { formatRelative } from '../../lib/format';
import { downloadJson } from '../../lib/exportGraph';
import { FormError, Modal, errorText } from '../Modal';
import { Button, Input, cx, focusRing } from '../ui';

interface Props {
  chains: ChainInfo[];
  chainId: number | null;
  canvasRange?: { from: number; to: number };
  onChange?: () => void;
  onClose: () => void;
}

const selectClass = cx(
  'h-[30px] max-w-[180px] cursor-pointer rounded-ui-sm border border-hairline bg-plane px-2 text-xs text-inherit hover:border-hairline-strong',
  focusRing,
);

function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function meterColor(confidence: number): string {
  if (confidence >= 0.75) return 'var(--critical)';
  if (confidence >= 0.4) return 'var(--accent)';
  return 'var(--text-muted)';
}

export function TlsDeanonDialog({ chains, chainId, canvasRange, onChange, onClose }: Props) {
  const [chain, setChain] = useState<number>(chainId ?? chains[0]?.chain_id ?? 1);
  const [links, setLinks] = useState<IpWalletLink[] | null>(null);
  const [run, setRun] = useState<CorrelateResult | null>(null);
  const [fromBlock, setFromBlock] = useState('');
  const [upToBlock, setUpToBlock] = useState('');
  const [view, setView] = useState<'ip' | 'wallet'>('ip');
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const chainName = (id: number) => chains.find((candidate) => candidate.chain_id === id)?.name ?? `Chain ${id}`;

  const reload = async (target: number) => {
    try {
      setLinks(await tlsApi.links(target));
    } catch (cause) {
      setError(errorText(cause));
      setLinks([]);
    }
  };

  useEffect(() => {
    setLinks(null);
    setRun(null);
    setError(null);
    void reload(chain);
  }, [chain]);

  const correlate = async (options: { reset?: boolean; from?: number; to?: number } = {}) => {
    setBusy(true);
    setError(null);
    try {
      const fromParsed = Number.parseInt(fromBlock, 10);
      const upToParsed = Number.parseInt(upToBlock, 10);
      setRun(
        await tlsApi.correlate(chain, {
          fromBlock: options.from ?? (Number.isFinite(fromParsed) ? fromParsed : undefined),
          upToBlock: options.to ?? (Number.isFinite(upToParsed) ? upToParsed : undefined),
          reset: options.reset,
        }),
      );
      await reload(chain);
      onChange?.();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  const reset = async () => {
    setBusy(true);
    setError(null);
    try {
      await tlsApi.reset(chain);
      setRun(null);
      await reload(chain);
      onChange?.();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  const exportLinks = () => {
    downloadJson(
      {
        kind: 'ledgerscope.tls_links',
        version: 1,
        exported_at: new Date().toISOString(),
        chain_id: chain,
        links: links ?? [],
      },
      `ledgerscope-tls-${chain}.json`,
    );
  };

  const needle = query.trim().toLowerCase();
  const filtered = (links ?? []).filter(
    (link) =>
      !needle ||
      link.src_ip.toLowerCase().includes(needle) ||
      link.address.toLowerCase().includes(needle),
  );
  const buckets = new Map<string, IpWalletLink[]>();
  for (const link of filtered) {
    const key = view === 'ip' ? link.src_ip : link.address;
    const bucket = buckets.get(key);
    if (bucket) bucket.push(link);
    else buckets.set(key, [link]);
  }
  const groups = [...buckets.entries()]
    .map(([key, items]) => ({
      key,
      items: [...items].sort((a, b) => b.score - a.score),
    }))
    .sort(
      (a, b) =>
        Math.max(...b.items.map((item) => item.score)) -
        Math.max(...a.items.map((item) => item.score)),
    );

  return (
    <Modal
      title="TLS deanonymization"
      subtitle="Links client IPs to wallets by correlating observer broadcast traffic with on-chain tx timing. Score = a wallet's matched tx / its total tx inside the IP's windows, so wallets active only when the IP sends rank high and always-busy addresses sink."
      onClose={onClose}
      width="lg"
    >
      <div className="grid gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <select
            className={selectClass}
            value={chain}
            aria-label="Chain"
            onChange={(event) => setChain(Number(event.target.value))}
          >
            {chains.map((candidate) => (
              <option key={candidate.chain_id} value={candidate.chain_id}>{candidate.name}</option>
            ))}
          </select>
          <Button
            variant="primary"
            disabled={busy || !canvasRange}
            title={canvasRange ? `Blocks ${canvasRange.from}–${canvasRange.to} on the canvas` : 'Open a graph first'}
            onClick={() => canvasRange && void correlate({ from: canvasRange.from, to: canvasRange.to })}
          >
            {busy ? 'Working…' : 'Correlate canvas range'}
          </Button>
          <Button variant="ghost" disabled={busy} onClick={() => void correlate()}>
            New blocks
          </Button>
          <Button variant="ghost" disabled={busy} onClick={() => void correlate({ reset: true })}>
            Recompute from scratch
          </Button>
          <Button variant="ghost" disabled={!links || links.length === 0} onClick={exportLinks}>
            Export JSON
          </Button>
          <Button variant="ghost" className="text-critical" disabled={busy} onClick={() => void reset()}>
            Clear
          </Button>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[11px] text-text-muted">Block range (optional, scope a public net)</span>
          <Input
            className="h-[30px] w-[120px] font-mono-ui text-xs"
            placeholder="from block"
            value={fromBlock}
            inputMode="numeric"
            spellCheck={false}
            onChange={(event) => setFromBlock(event.target.value)}
          />
          <Input
            className="h-[30px] w-[120px] font-mono-ui text-xs"
            placeholder="to block (head)"
            value={upToBlock}
            inputMode="numeric"
            spellCheck={false}
            onChange={(event) => setUpToBlock(event.target.value)}
          />
        </div>

        {run && (
          <p className="m-0 text-[11px] text-text-muted">
            Pass over blocks <BlockSpanText from={run.from_block} to={run.to_block} /> on {chainName(chain)}: {run.matched_pairs} timing matches, {run.links_touched} links touched.
          </p>
        )}

        <FormError message={error} />

        <div className="flex flex-wrap items-center gap-3 text-[11px] text-text-muted">
          <span>Score (dots on nodes):</span>
          <span className="inline-flex items-center gap-1"><span className="size-2 rounded-full" style={{ background: '#ef4444' }} /> ≥ 75%</span>
          <span className="inline-flex items-center gap-1"><span className="size-2 rounded-full" style={{ background: '#f59e0b' }} /> 40–75%</span>
          <span className="inline-flex items-center gap-1"><span className="size-2 rounded-full" style={{ background: '#9ca3af' }} /> &lt; 40%</span>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="flex rounded-ui-sm border border-hairline bg-plane p-0.5" role="radiogroup" aria-label="Group by">
            {(['ip', 'wallet'] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                role="radio"
                aria-checked={view === mode}
                className={cx(
                  'h-6 cursor-pointer rounded-[4px] px-2.5 text-[11px] font-medium [transition:background_120ms_ease,color_120ms_ease]',
                  view === mode ? 'bg-surface-2 text-text-primary shadow-[0_1px_2px_rgba(0,0,0,0.12)]' : 'text-text-muted hover:text-text-primary',
                  focusRing,
                )}
                onClick={() => setView(mode)}
              >
                {mode === 'ip' ? 'By IP' : 'By wallet'}
              </button>
            ))}
          </div>
          <Input
            className="h-[30px] min-w-[200px] flex-1 font-mono-ui text-xs"
            placeholder="filter by ip or address"
            value={query}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>

        {links === null && <p className="m-0 text-[13px] text-text-muted">Loading…</p>}
        {links !== null && groups.length === 0 && (
          <p className="m-0 py-6 text-center text-[13px] text-text-muted">
            {links.length === 0
              ? 'No links yet. Point the observer at your rpc node, scan some blocks, then correlate.'
              : 'Nothing matches the filter.'}
          </p>
        )}

        {groups.map((group) => (
          <div key={group.key} className="grid gap-0.5">
            <div className="flex items-center gap-2 px-2.5 pt-1.5 text-xs">
              <span className="text-[10px] font-semibold uppercase tracking-wide text-text-muted">
                {view === 'ip' ? 'IP' : 'Wallet'}
              </span>
              <span className="truncate font-mono-ui font-medium" title={group.key}>{group.key}</span>
              <span className="text-[10px] font-medium text-text-muted">{group.items.length}</span>
            </div>
            {group.items.map((link) => {
              const other = view === 'ip' ? link.address : link.src_ip;
              return (
                <div
                  key={`${link.src_ip}-${link.address}`}
                  className="grid grid-cols-[1fr_120px_64px_90px] items-center gap-3 rounded-ui-sm px-2.5 py-1.5 hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]"
                >
                  <div className="truncate pl-3 font-mono-ui text-xs text-text-secondary" title={other}>{other}</div>
                  <div className="flex items-center gap-2">
                    <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-[color-mix(in_srgb,var(--text-primary)_10%,transparent)]">
                      <div
                        className="h-full rounded-full"
                        style={{ width: percent(link.score), background: meterColor(link.score) }}
                      />
                    </div>
                    <span className="w-9 text-right text-[11px] tabular-nums text-text-muted">{percent(link.score)}</span>
                  </div>
                  <div
                    className="text-right text-xs tabular-nums text-text-muted"
                    title="matched tx / total tx of this address in the windows"
                  >
                    {link.hits}/{link.address_txs}
                  </div>
                  <div className="text-right text-[11px] text-text-muted">{formatRelative(link.last_seen)}</div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </Modal>
  );
}
