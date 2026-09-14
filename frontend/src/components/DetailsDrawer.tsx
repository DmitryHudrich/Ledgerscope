import { useMemo, useState } from 'react';

import {
  formatCount,
  formatEth,
  formatRelative,
  formatTimestamp,
  shortAddress,
  weiToEth,
} from '../lib/format';
import type { GraphModel, GraphNode } from '../graph/model';
import { IconClose, IconCopy, IconTable, IconTarget } from './Icons';

interface Props {
  node: GraphNode;
  model: GraphModel;
  onSelect: (id: string | null) => void;
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
}

const PEER_PREVIEW = 10;

export function DetailsDrawer({
  node,
  model,
  onSelect,
  onCenter,
  onShowTransactions,
  onClose,
}: Props) {
  const [showAllPeers, setShowAllPeers] = useState(false);
  const [copied, setCopied] = useState(false);

  const peers = useMemo<Peer[]>(() => {
    const byId = new Map<string, Peer>();
    for (const link of model.linksByNode.get(node.id) ?? []) {
      const outgoing = link.source.id === node.id;
      const other = outgoing ? link.target : link.source;
      let peer = byId.get(other.id);
      if (!peer) {
        peer = { id: other.id, kind: other.kind, received: 0n, sent: 0n, txs: 0 };
        byId.set(other.id, peer);
      }
      peer.txs += link.count;
      if (outgoing) peer.sent += link.value;
      else peer.received += link.value;
    }
    return [...byId.values()].sort((a, b) =>
      Number(weiToEth(b.received + b.sent) - weiToEth(a.received + a.sent)),
    );
  }, [model, node.id]);

  const inEth = weiToEth(node.valueIn);
  const outEth = weiToEth(node.valueOut);
  const total = inEth + outEth;
  const inShare = total > 0 ? (inEth / total) * 100 : 50;

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

  return (
    <aside className="panel overlay drawer" aria-label="Address details">
      <header className="drawer-head">
        <div className="drawer-title">
          <span className="kind-chip">
            <span className={`dot dot-${node.kind}`} aria-hidden="true" />
            {KIND_LABEL[node.kind]}
          </span>
          <div className="address">{node.id}</div>
        </div>
        <button
          type="button"
          className="btn btn-ghost btn-icon"
          onClick={onClose}
          aria-label="Close details"
          title="Close"
        >
          <IconClose />
        </button>
      </header>

      <div className="drawer-body">
        <section className="panel-section">
          <div className="stat-grid">
            <div className="stat">
              <div className="stat-label">Received</div>
              <div className="stat-value">
                {formatEth(node.valueIn)}
                <span className="stat-unit">ETH</span>
              </div>
            </div>
            <div className="stat">
              <div className="stat-label">Sent</div>
              <div className="stat-value">
                {formatEth(node.valueOut)}
                <span className="stat-unit">ETH</span>
              </div>
            </div>
          </div>

          <div className="flow-bar" aria-hidden="true">
            <span
              style={{
                width: `${inShare}%`,
                background: `var(--series-${node.kind === 'focus' ? 'focus' : node.kind})`,
              }}
            />
            <span
              style={{
                width: `${100 - inShare}%`,
                background: `color-mix(in srgb, var(--series-${
                  node.kind === 'focus' ? 'focus' : node.kind
                }) 35%, transparent)`,
              }}
            />
          </div>

          <dl className="meta-list">
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
          </dl>

          <div className="button-row">
            <button type="button" className="btn" onClick={() => onCenter(node.id)}>
              <IconTarget /> Center
            </button>
            <button type="button" className="btn" onClick={onShowTransactions}>
              <IconTable /> Transactions
            </button>
            <button type="button" className="btn" onClick={copy}>
              <IconCopy /> {copied ? 'Copied' : 'Copy'}
            </button>
          </div>
        </section>

        <section className="panel-section">
          <h2 className="panel-title">
            Counterparties · {formatCount(peers.length)}
          </h2>
          <div className="peer-list">
            {visiblePeers.map((peer) => {
              const net = peer.received - peer.sent;
              return (
                <button
                  key={peer.id}
                  type="button"
                  className="peer"
                  onClick={() => onSelect(peer.id)}
                  title={peer.id}
                >
                  <span className={`dot dot-${peer.kind}`} aria-hidden="true" />
                  <span className="peer-address">{shortAddress(peer.id, 10, 6)}</span>
                  <span className="peer-value">
                    {net >= 0n ? '+' : '−'}
                    {formatEth(net >= 0n ? net : -net)} ETH
                  </span>
                </button>
              );
            })}
          </div>
          {peers.length > PEER_PREVIEW && (
            <button
              type="button"
              className="btn btn-ghost"
              style={{ marginTop: 8 }}
              onClick={() => setShowAllPeers((value) => !value)}
            >
              {showAllPeers ? 'Show top 10' : `Show all ${formatCount(peers.length)}`}
            </button>
          )}
        </section>
      </div>
    </aside>
  );
}
