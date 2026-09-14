import type { GraphResponse, GraphQuery, TxResponse } from './types';

/**
 * Deterministic sample payload so the UI is explorable without an ETH_RPC_URL
 * and a live node. Shaped like real flow: a focus wallet, a few exchange-style
 * hubs, a contract cluster reached through calldata, and peripheral EOAs.
 */

const ERC20_TRANSFER = '0xa9059cbb';

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function makeAddress(rand: () => number): string {
  let hex = '';
  for (let i = 0; i < 40; i += 1) hex += Math.floor(rand() * 16).toString(16);
  return `0x${hex}`;
}

function makeHash(rand: () => number): string {
  let hex = '';
  for (let i = 0; i < 64; i += 1) hex += Math.floor(rand() * 16).toString(16);
  return `0x${hex}`;
}

/** ETH amount -> wei as a decimal string, without floating point drift. */
function eth(amount: number): string {
  const wei = BigInt(Math.round(amount * 1e6)) * 10n ** 12n;
  return wei.toString();
}

function calldata(rand: () => number): string {
  let hex = ERC20_TRANSFER;
  for (let i = 0; i < 128; i += 1) hex += Math.floor(rand() * 16).toString(16);
  return hex;
}

export function demoGraph(query: GraphQuery): GraphResponse {
  const rand = mulberry32(0x1ede_a501);
  const fromBlock = query.from;
  const toBlock = Math.max(query.to, query.from + 1);
  const span = toBlock - fromBlock;
  // ~12s per block, anchored so the newest tx lands near "now".
  const baseTime = Math.floor(Date.now() / 1000) - span * 12;

  const focus = /^0x[0-9a-fA-F]{40}$/.test(query.wallet)
    ? query.wallet.toLowerCase()
    : makeAddress(rand);

  const hubs = Array.from({ length: 4 }, () => makeAddress(rand));
  const contracts = Array.from({ length: 5 }, () => makeAddress(rand));
  const peers = Array.from({ length: 34 }, () => makeAddress(rand));

  const nodes = new Set<string>([focus, ...hubs, ...contracts, ...peers]);
  const edges: TxResponse[] = [];

  const push = (from: string, to: string, amount: number, withData: boolean) => {
    const blockNumber = fromBlock + Math.floor(rand() * span);
    edges.push({
      tx_hash: makeHash(rand),
      block_number: blockNumber,
      timestamp: baseTime + (blockNumber - fromBlock) * 12,
      amount: eth(amount),
      from,
      to,
      data: withData ? calldata(rand) : '0x',
    });
  };

  // Focus wallet funded by, and cashing out to, the hubs.
  for (const hub of hubs) {
    const inbound = 1 + Math.floor(rand() * 4);
    for (let i = 0; i < inbound; i += 1) push(hub, focus, 0.4 + rand() * 18, false);
    if (rand() > 0.35) push(focus, hub, 0.2 + rand() * 9, false);
  }

  // Focus wallet interacting with contracts (calldata present -> contract node).
  for (const contract of contracts) {
    push(focus, contract, rand() * 2.5, true);
    if (rand() > 0.5) push(focus, contract, rand() * 1.2, true);
  }

  // Peripheral fan-out: peers trade with hubs and with each other.
  peers.forEach((peer, index) => {
    const hub = hubs[index % hubs.length];
    push(hub, peer, 0.05 + rand() * 3.5, false);
    if (rand() > 0.45) push(peer, hub, 0.05 + rand() * 2.2, false);
    if (rand() > 0.6) push(peer, contracts[index % contracts.length], rand() * 0.8, true);
    if (rand() > 0.7) push(peer, peers[(index + 7) % peers.length], rand() * 1.4, false);
    if (rand() > 0.85) push(focus, peer, rand() * 0.6, false);
  });

  // A short peel-off chain — the shape analysts actually look for.
  let cursor = focus;
  for (let hop = 0; hop < 5; hop += 1) {
    const next = makeAddress(rand);
    nodes.add(next);
    push(cursor, next, 12 - hop * 1.8, false);
    cursor = next;
  }
  push(cursor, hubs[0], 3.1, false);

  edges.sort((a, b) => a.block_number - b.block_number);
  return { nodes: [...nodes], edges };
}
