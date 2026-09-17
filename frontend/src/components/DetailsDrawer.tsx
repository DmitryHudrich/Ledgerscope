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
import { IconClose, IconCopy, IconTable, IconTarget } from './Icons';

interface Props {
  node: GraphNode;
  model: GraphModel;
  rooted: boolean;
  onSelect: (id: string | null) => void;
  onCenter: (id: string) => void;
  onExpand: () => void;
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
  rooted,
  onSelect,
  onCenter,
  onExpand,
  onShowTransactions,
  onClose,
}: Props) {
  const [showAllPeers, setShowAllPeers] = useState(false);
  const [showAllAssets, setShowAllAssets] = useState(false);
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
  const visibleAssets = showAllAssets ? assets : assets.slice(0, ASSET_PREVIEW);

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
              <div className="stat-label">Received (ETH)</div>
              <div className="stat-value">
                {formatEth(node.valueIn)}
                <span className="stat-unit">ETH</span>
              </div>
            </div>
            <div className="stat">
              <div className="stat-label">Sent (ETH)</div>
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
            <button
              type="button"
              className="btn btn-primary"
              disabled={rooted}
              title={
                rooted
                  ? 'Already an investigation root'
                  : 'Add this address to the canvas and walk the graph from it'
              }
              onClick={onExpand}
            >
              <IconTarget /> {rooted ? 'Is a root' : 'Build from here'}
            </button>
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

        {assets.length > 0 && (
          <section className="panel-section">
            <h2 className="panel-title">Assets · {formatCount(assets.length)}</h2>
            <table className="asset-table">
              <thead>
                <tr>
                  <th>Asset</th>
                  <th className="num">In</th>
                  <th className="num">Out</th>
                  <th className="num">Tx</th>
                </tr>
              </thead>
              <tbody>
                {visibleAssets.map((flow) => (
                  <tr key={flow.key}>
                    <td title={flow.native ? 'ETH' : flow.key}>
                      <span
                        className={`dot dot-${flow.native ? 'focus' : 'token'}`}
                        aria-hidden="true"
                      />
                      <span className="asset-symbol">{assetLabel(flow)}</span>
                    </td>
                    <td className="num">{formatUnits(flow.received, flow.decimals)}</td>
                    <td className="num">{formatUnits(flow.sent, flow.decimals)}</td>
                    <td className="num">{formatCount(flow.count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {assets.length > ASSET_PREVIEW && (
              <button
                type="button"
                className="btn btn-ghost"
                style={{ marginTop: 8 }}
                onClick={() => setShowAllAssets((value) => !value)}
              >
                {showAllAssets ? `Show top ${ASSET_PREVIEW}` : `Show all ${formatCount(assets.length)}`}
              </button>
            )}
          </section>
        )}

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
