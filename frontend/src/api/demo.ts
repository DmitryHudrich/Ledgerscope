import type { GraphEdge, GraphQuery, GraphResponse, Interaction, TxMeta } from './types';

const TOKENS: Array<{ symbol: string; decimals: number }> = [
  { symbol: 'USDC', decimals: 6 },
  { symbol: 'USDT', decimals: 6 },
  { symbol: 'DAI', decimals: 18 },
  { symbol: 'WETH', decimals: 18 },
  { symbol: 'LINK', decimals: 18 },
];

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

function eth(amount: number): string {
  const wei = BigInt(Math.round(amount * 1e6)) * 10n ** 12n;
  return wei.toString();
}

export function demoGraph(query: GraphQuery): GraphResponse {
  const rand = mulberry32(0x1ede_a501);
  const fromBlock = query.from;
  const toBlock = Math.max(query.to, query.from + 1);
  const span = toBlock - fromBlock;

  const baseTime = Math.floor(Date.now() / 1000) - span * 12;

  const focus = /^0x[0-9a-fA-F]{40}$/.test(query.wallet)
    ? query.wallet.toLowerCase()
    : makeAddress(rand);

  const hubs = Array.from({ length: 4 }, () => makeAddress(rand));
  const contracts = Array.from({ length: 5 }, () => makeAddress(rand));
  const tokens = TOKENS.map((token) => ({ ...token, address: makeAddress(rand) }));
  const peers = Array.from({ length: 34 }, () => makeAddress(rand));

  const nodes = new Set<string>([focus, ...hubs, ...contracts, ...peers]);
  const edges: GraphEdge[] = [];

  const meta = (): TxMeta => {
    const blockNumber = fromBlock + Math.floor(rand() * span);
    return {
      tx_hash: makeHash(rand),
      block_number: blockNumber,
      timestamp: baseTime + (blockNumber - fromBlock) * 12,
      succeeded: rand() > 0.06,
    };
  };

  const emit = (interaction: Interaction) => {
    edges.push({ ...meta(), ...interaction });
  };

  const transfer = (from: string, to: string, amount: number) =>
    emit({ kind: 'native_transfer', from, to, amount: eth(amount) });

  const call = (interactor: string, contract: string) =>
    emit({
      kind: 'contract_interaction',
      interactor,
      contract_address: contract,
      action: { kind: 'other' },
    });

  const tokenTransfer = (from: string, to: string, contract: string, units: number) => {
    const token = tokens[Math.floor(rand() * tokens.length)];
    const raw =
      BigInt(Math.max(1, Math.round(units * 1000))) * 10n ** BigInt(token.decimals) / 1000n;
    emit({
      kind: 'contract_interaction',
      interactor: from,
      contract_address: contract,
      action: {
        kind: 'erc20_transfer',
        token: token.address,
        from,
        to,
        amount: raw.toString(),
        token_name: token.symbol,
        decimals: token.decimals,
      },
    });
  };

  for (const contract of contracts) {
    emit({
      kind: 'contract_deployment',
      deployer: hubs[Math.floor(rand() * hubs.length)],
      contract_address: contract,
    });
  }

  for (const hub of hubs) {
    const inbound = 1 + Math.floor(rand() * 4);
    for (let i = 0; i < inbound; i += 1) transfer(hub, focus, 0.4 + rand() * 18);
    if (rand() > 0.35) transfer(focus, hub, 0.2 + rand() * 9);
  }

  for (const contract of contracts) {
    call(focus, contract);
    if (rand() > 0.5) {
      tokenTransfer(focus, peers[Math.floor(rand() * peers.length)], contract, rand() * 25000);
    }
  }

  peers.forEach((peer, index) => {
    const hub = hubs[index % hubs.length];
    transfer(hub, peer, 0.05 + rand() * 3.5);
    if (rand() > 0.45) transfer(peer, hub, 0.05 + rand() * 2.2);
    if (rand() > 0.6) call(peer, contracts[index % contracts.length]);
    if (rand() > 0.7) transfer(peer, peers[(index + 7) % peers.length], rand() * 1.4);
    if (rand() > 0.8) {
      tokenTransfer(peer, hub, contracts[(index + 2) % contracts.length], rand() * 8000);
    }
    if (rand() > 0.85) transfer(focus, peer, rand() * 0.6);
  });

  let cursor = focus;
  for (let hop = 0; hop < 5; hop += 1) {
    const next = makeAddress(rand);
    nodes.add(next);
    transfer(cursor, next, 12 - hop * 1.8);
    cursor = next;
  }
  transfer(cursor, hubs[0], 3.1);

  edges.sort((a, b) => a.block_number - b.block_number);
  return { nodes: [...nodes], edges };
}
