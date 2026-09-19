import { useMemo, useState } from 'react';

import {
  formatCount,
  formatEth,
  formatRelative,
  formatTimestamp,
  formatUnits,
  shortAddress,
  weiToEth,
} from '../lib/format';
import { assetLabel, sortedAssets, type GraphModel, type GraphNode } from '../graph/model';
import { IconChevron, IconClose, IconCopy, IconTable, IconTarget } from './Icons';
import {
  Button,
  Dot,
  MetaList,
  Panel,
  PanelSection,
  PanelTitle,
  cx,
  focusRing,
} from './ui';

interface Props {
  node: GraphNode;
  model: GraphModel;
  onSelect: (id: string | null) => void;
  onBuildFromHere: () => void;
  buildingFromHere: boolean;
  buildFromHereDisabled: boolean;
  buildFromHereHint: string | null;
  buildFeedback: { kind: 'success' | 'info' | 'error'; message: string } | null;
  onCenter: (id: string) => void;
  onShowTransactions: () => void;
  onClose: () => void;
}

const KIND_LABEL: Record<GraphNode['kind'], string> = {
  focus: 'Focus wallet',
  contract: 'Contract',
  eoa: 'Wallet (EOA)',
};

interface Peer {
  id: string;
  kind: GraphNode['kind'];
  received: bigint;
  sent: bigint;
  txs: number;
  hasEth: boolean;
}

const PEER_PREVIEW = 10;
const ASSET_PREVIEW = 8;

export function DetailsDrawer({
  node,
  model,
  onSelect,
  onBuildFromHere,
  buildingFromHere,
  buildFromHereDisabled,
  buildFromHereHint,
  buildFeedback,
  onCenter,
  onShowTransactions,
  onClose,
}: Props) {
  const [showAllPeers, setShowAllPeers] = useState(false);
  const [showAllAssets, setShowAllAssets] = useState(false);
  const [peersOpen, setPeersOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  const peers = useMemo<Peer[]>(() => {
    const byId = new Map<string, Peer>();
    for (const link of model.linksByNode.get(node.id) ?? []) {
      const outgoing = link.source.id === node.id;
      const other = outgoing ? link.target : link.source;
      let peer = byId.get(other.id);
      if (!peer) {
        peer = {
          id: other.id,
          kind: other.kind,
          received: 0n,
          sent: 0n,
          txs: 0,
          hasEth: false,
        };
        byId.set(other.id, peer);
      }
      peer.txs += link.count;
      if (link.value > 0n) peer.hasEth = true;
      if (outgoing) peer.sent += link.value;
      else peer.received += link.value;
    }
    return [...byId.values()].sort((a, b) => {
      const delta = weiToEth(b.received + b.sent) - weiToEth(a.received + a.sent);
      return delta !== 0 ? delta : b.txs - a.txs;
    });
  }, [model, node.id]);

  const assets = useMemo(() => {
    const keys = new Set([...node.assetsIn.keys(), ...node.assetsOut.keys()]);
    const rows = [...keys].map((key) => {
      const inflow = node.assetsIn.get(key);
      const outflow = node.assetsOut.get(key);
      const sample = (inflow ?? outflow)!;
      return {
        key,
        symbol: sample.symbol,
        decimals: sample.decimals,
        native: sample.native,
        amount: (inflow?.amount ?? 0n) + (outflow?.amount ?? 0n),
        count: (inflow?.count ?? 0) + (outflow?.count ?? 0),
        received: inflow?.amount ?? 0n,
        sent: outflow?.amount ?? 0n,
      };
    });
    const order = new Map(sortedAssets(rows).map((flow, index) => [flow.key, index]));
    return rows.sort((a, b) => (order.get(a.key) ?? 0) - (order.get(b.key) ?? 0));
  }, [node]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(node.id);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      setCopied(false);
    }
  };

  const visiblePeers = showAllPeers ? peers : peers.slice(0, PEER_PREVIEW);
  const visibleAssets = showAllAssets ? assets : assets.slice(0, ASSET_PREVIEW);

  return (
    <Panel
      as="aside"
      className="absolute bottom-[var(--sheet-h)] right-0 top-0 z-10 flex w-[var(--inspector-w)] animate-drawer-in flex-col overflow-hidden bg-surface-1 shadow-none backdrop-blur-none transition-[bottom,width] duration-200 ease-[cubic-bezier(0.22,0.61,0.36,1)]"
      style={{ borderRadius: 0, borderWidth: 0, borderLeftWidth: 1 }}
      aria-label="Address details"
    >
      <header className="flex items-start gap-3 border-b border-hairline px-4 py-[14px]">
        <div className="min-w-0 flex-1">
          <span className="inline-flex items-center gap-[6px] text-[10px] uppercase tracking-[0.07em] text-text-secondary">
            <Dot tone={node.kind} />
            {KIND_LABEL[node.kind]}
          </span>
          <div className="mt-1.5 break-all font-mono-ui text-[12.5px] leading-relaxed">{node.id}</div>
        </div>
        <Button
          variant="ghost"
          icon
          onClick={onClose}
          aria-label="Close details"
          title="Close"
        >
          <IconClose />
        </Button>
      </header>

      <div className="overflow-y-auto overscroll-contain">
        <PanelSection>
          <PanelTitle>Assets · {formatCount(assets.length)}</PanelTitle>
          {assets.length > 0 ? (
            <>
              <table className="w-full border-collapse text-xs [&_td]:border-t [&_td]:border-hairline [&_td]:py-[6px] [&_td:first-child]:whitespace-nowrap [&_th]:pb-1.5 [&_th]:text-left [&_th]:text-[10px] [&_th]:font-medium [&_th]:uppercase [&_th]:tracking-[0.06em] [&_th]:text-text-muted [&_.num]:text-right [&_.num]:tabular-nums">
                <thead>
                  <tr>
                    <th>Asset</th>
                    <th className="num">Received</th>
                    <th className="num">Sent</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleAssets.map((flow) => (
                    <tr key={flow.key}>
                      <td title={flow.native ? 'ETH' : flow.key}>
                        <Dot tone={flow.native ? 'focus' : 'token'} className="mr-[6px] align-middle" />
                        <span className="inline-block max-w-[92px] overflow-hidden text-ellipsis whitespace-nowrap align-middle font-mono-ui">
                          {assetLabel(flow)}
                        </span>
                      </td>
                      <td className="num">{formatUnits(flow.received, flow.decimals)}</td>
                      <td className="num">{formatUnits(flow.sent, flow.decimals)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {assets.length > ASSET_PREVIEW && (
                <Button variant="ghost" className="mt-2" onClick={() => setShowAllAssets((value) => !value)}>
                  {showAllAssets ? `Show top ${ASSET_PREVIEW}` : `Show all ${formatCount(assets.length)}`}
                </Button>
              )}
            </>
          ) : (
            <p className="m-0 text-xs text-text-muted">No transferred assets for this address.</p>
          )}
        </PanelSection>

        <PanelSection>
          <PanelTitle>Activity</PanelTitle>
          <MetaList>
            <div>
              <dt>Transactions</dt>
              <dd>
                {formatCount(node.inCount)} in · {formatCount(node.outCount)} out
              </dd>
            </div>
            <div>
              <dt>Counterparties</dt>
              <dd>{formatCount(node.degree)}</dd>
            </div>
            <div>
              <dt>Blocks</dt>
              <dd>
                {node.firstBlock
                  ? `${formatCount(node.firstBlock)} – ${formatCount(node.lastBlock)}`
                  : '—'}
              </dd>
            </div>
            <div>
              <dt>First seen</dt>
              <dd title={formatTimestamp(node.firstSeen)}>{formatRelative(node.firstSeen)}</dd>
            </div>
            <div>
              <dt>Last seen</dt>
              <dd title={formatTimestamp(node.lastSeen)}>{formatRelative(node.lastSeen)}</dd>
            </div>
          </MetaList>

          <div className="mt-4">
            <Button
              variant="primary"
              className="w-full"
              disabled={buildingFromHere || buildFromHereDisabled}
              title={buildFromHereHint ?? undefined}
              onClick={onBuildFromHere}
            >
              {buildingFromHere ? 'Building…' : 'Build from here'}
            </Button>
            {buildFromHereHint && (
              <p className="mb-0 mt-[6px] text-[11px] text-text-muted">
                {buildFromHereHint}
              </p>
            )}
            {buildFeedback && (
              <p
                className={cx(
                  'mb-0 mt-[6px] text-[11px]',
                  buildFeedback.kind === 'error'
                    ? 'text-critical'
                    : buildFeedback.kind === 'success'
                      ? 'text-good'
                      : 'text-text-muted',
                )}
                role={buildFeedback.kind === 'error' ? 'alert' : 'status'}
              >
                {buildFeedback.message}
              </p>
            )}
            <div className="mt-2 grid grid-cols-3 gap-[6px]">
              <Button variant="ghost" className="px-2" onClick={() => onCenter(node.id)}>
                <IconTarget /> Center
              </Button>
              <Button variant="ghost" className="px-2" onClick={onShowTransactions}>
                <IconTable /> Transactions
              </Button>
              <Button variant="ghost" className="px-2" onClick={copy}>
                <IconCopy /> {copied ? 'Copied' : 'Copy'}
              </Button>
            </div>
          </div>
        </PanelSection>

        <PanelSection className="p-0">
          <button
            type="button"
            className={cx(
              'flex w-full cursor-pointer items-center justify-between border-0 bg-transparent px-[14px] py-3 text-left hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]',
              focusRing,
            )}
            onClick={() => setPeersOpen((value) => !value)}
            aria-expanded={peersOpen}
          >
            <span className="text-[10px] font-semibold uppercase tracking-[0.09em] text-text-muted">
              Counterparties · {formatCount(peers.length)}
            </span>
            <IconChevron className={cx('transition-transform duration-150', peersOpen && 'rotate-180')} />
          </button>
          {peersOpen && <div className="grid gap-px px-[6px] pb-3">
            {visiblePeers.map((peer) => {
              const net = peer.received - peer.sent;
              return (
                <button
                  key={peer.id}
                  type="button"
                  className={cx(
                    'grid w-full cursor-pointer grid-cols-[auto_1fr_auto] items-center gap-2 rounded-ui-sm border-0 bg-transparent px-2 py-[7px] text-left hover:bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)]',
                    focusRing,
                  )}
                  onClick={() => onSelect(peer.id)}
                  title={peer.id}
                >
                  <Dot tone={peer.kind} />
                  <span className="overflow-hidden text-ellipsis font-mono-ui text-xs">
                    {shortAddress(peer.id, 10, 6)}
                  </span>
                  <span className="text-xs tabular-nums text-text-secondary">
                    {peer.hasEth ? (
                      <>
                        {net >= 0n ? '+' : '−'}
                        {formatEth(net >= 0n ? net : -net)} ETH
                      </>
                    ) : (
                      `${formatCount(peer.txs)} tx`
                    )}
                  </span>
                </button>
              );
            })}
          </div>}
          {peersOpen && peers.length > PEER_PREVIEW && (
            <Button
              variant="ghost"
              className="mb-3 ml-3"
              onClick={() => setShowAllPeers((value) => !value)}
            >
              {showAllPeers ? 'Show top 10' : `Show all ${formatCount(peers.length)}`}
            </Button>
          )}
        </PanelSection>
      </div>
    </Panel>
  );
}
