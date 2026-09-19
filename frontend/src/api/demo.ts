import type { GraphEdge, GraphQuery, GraphResponse, Interaction, TxMeta } from './types';

const TOKENS = [
  { symbol: 'USDC', decimals: 6 },
  { symbol: 'WETH', decimals: 18 },
  { symbol: 'LINK', decimals: 18 },
] as const;

function mulberry32(seed: number): () => number {
  let value = seed >>> 0;
  return () => {
    value = (value + 0x6d2b79f5) >>> 0;
    let mixed = Math.imul(value ^ (value >>> 15), 1 | value);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

function hex(rand: () => number, length: number): string {
  let value = '';
  for (let index = 0; index < length; index += 1) {
    value += Math.floor(rand() * 16).toString(16);
  }
  return `0x${value}`;
}

function units(amount: number, decimals = 18): string {
  return (BigInt(Math.round(amount * 1e6)) * 10n ** BigInt(decimals - 6)).toString();
}

export function demoGraph(query: GraphQuery): GraphResponse {
  const rand = mulberry32(0x1edea501);
  const from = query.from_block;
  const to = Math.max(query.to_block, from);
  const blockSpan = Math.max(to - from + 1, 1);
  const roots = query.roots.length > 0
    ? query.roots
    : [{ address: hex(rand, 40), depth: 1 }];
  const peers = Array.from({ length: 34 }, () => hex(rand, 40));
  const contracts = Array.from({ length: 5 }, () => hex(rand, 40));
  const tokenAddresses = TOKENS.map(() => hex(rand, 40));
  const addresses = new Set([
    ...roots.map((root) => root.address.toLowerCase()),
    ...peers,
    ...contracts,
  ]);
  const edges: GraphEdge[] = [];
  const baseTime = 1_729_345_547;

  const meta = (): TxMeta => {
    const offset = Math.floor(rand() * blockSpan);
    return {
      tx_hash: hex(rand, 64),
      block_number: from + offset,
      timestamp: baseTime + offset * 12,
      succeeded: rand() > 0.06,
    };
  };
  const emit = (interaction: Interaction) => edges.push({ ...meta(), ...interaction });
  const pick = <T,>(items: T[]): T => items[Math.floor(rand() * items.length)]!;

  for (const root of roots) {
    const address = root.address.toLowerCase();
    for (let index = 0; index < 18; index += 1) {
      const peer = pick(peers);
      const outgoing = rand() > 0.42;
      emit({
        kind: 'native_transfer',
        from: outgoing ? address : peer,
        to: outgoing ? peer : address,
        amount: units(0.02 + rand() * 8),
      });
    }
  }

  for (let index = 0; index < 42; index += 1) {
    const fromAddress = rand() > 0.55 ? pick(roots).address.toLowerCase() : pick(peers);
    const toAddress = pick(peers);
    const tokenIndex = Math.floor(rand() * TOKENS.length);
    const token = TOKENS[tokenIndex]!;
    emit({
      kind: 'contract_interaction',
      interactor: fromAddress,
      contract_address: pick(contracts),
      action: {
        kind: 'erc20_transfer',
        token: tokenAddresses[tokenIndex]!,
        from: fromAddress,
        to: toAddress,
        amount: units(5 + rand() * 2_000, token.decimals),
        token_name: token.symbol,
        decimals: token.decimals,
      },
    });
  }

  for (let index = 0; index < 12; index += 1) {
    emit({
      kind: 'contract_interaction',
      interactor: pick(peers),
      contract_address: pick(contracts),
      action: { kind: 'other' },
    });
  }

  return {
    nodes: [...addresses].map((address) => {
      const root = roots.find((candidate) => candidate.address.toLowerCase() === address);
      return {
        address,
        depth: root?.depth ?? 1,
        root: Boolean(root),
        expanded: Boolean(root),
      };
    }),
    edges,
    span: { from_block: from, to_block: to, block_count: blockSpan },
    filled_from_rpc: [],
    truncated: false,
  };
}
