import { edgeEndpoints, edgeFlavor, edgeTransfer } from '../api/edges';
import type { ChainInfo, GraphRoot } from '../api/types';
import type { GraphModel } from '../graph/model';
import type { BlockSpan } from './blockRange';
import { displayAddress } from './chain';
import { groupStats, type GraphGroup } from './groups';

const amount = (value: bigint) => value.toString();

export function graphExport(options: {
  model: GraphModel;
  chain: ChainInfo | null;
  roots: GraphRoot[];
  selection: BlockSpan;
  groups: GraphGroup[];
  hiddenNodes: Set<string>;
  hiddenTxs: Set<string>;
  allEdges: Parameters<typeof groupStats>[1];
}) {
  const { model, chain, roots, selection, groups, hiddenNodes, hiddenTxs, allEdges } = options;
  const groupsOfNode = new Map<string, string[]>();
  const groupsOfTx = new Map<string, string[]>();
  for (const group of groups) {
    for (const id of group.nodes) groupsOfNode.set(id, [...(groupsOfNode.get(id) ?? []), group.name]);
    for (const hash of group.txs) groupsOfTx.set(hash, [...(groupsOfTx.get(hash) ?? []), group.name]);
  }

  const assets = (flows: GraphModel['nodes'][number]['assetsIn']) =>
    [...flows.values()].map((flow) => ({ asset: flow.key, symbol: flow.symbol, decimals: flow.decimals, amount: amount(flow.amount) }));

  return {
    kind: 'ledgerscope.graph',
    version: 1,
    exported_at: new Date().toISOString(),
    chain: chain ? { id: chain.chain_id, name: chain.name, family: chain.family, native_symbol: chain.native_symbol } : null,
    span: { from_block: selection.from, to_block: selection.to },
    roots: roots.map((root) => ({ address: root.address, display_address: displayAddress(root.address), depth: root.depth })),
    nodes: model.nodes.map((node) => ({
      address: node.id,
      display_address: displayAddress(node.id),
      kind: node.kind,
      labels: node.systemLabels.map((label) => ({ value: label.value, source: label.source })),
      groups: groupsOfNode.get(node.id) ?? [],
      depth: node.depth,
      transactions_in: node.inCount,
      transactions_out: node.outCount,
      first_block: node.firstBlock,
      last_block: node.lastBlock,
      received: assets(node.assetsIn),
      sent: assets(node.assetsOut),
    })),
    transactions: model.links.flatMap((link) => link.txs.map((tx) => {
      const ends = edgeEndpoints(tx);
      const transfer = edgeTransfer(tx);
      return {
        tx_hash: tx.tx_hash,
        block_number: tx.block_number,
        timestamp: tx.timestamp,
        succeeded: tx.succeeded,
        type: edgeFlavor(tx),
        from: ends?.from ?? null,
        to: ends?.to ?? null,
        asset: transfer?.key ?? null,
        symbol: transfer?.symbol ?? null,
        decimals: transfer?.decimals ?? null,
        amount: transfer ? amount(transfer.amount) : null,
        groups: groupsOfTx.get(tx.tx_hash.toLowerCase()) ?? [],
      };
    })),
    groups: groups.map((group) => {
      const stats = groupStats(group, allEdges);
      return {
        name: group.name,
        color: group.color,
        hidden: group.hidden,
        nodes: group.nodes,
        transactions: group.txs,
        stats: {
          nodes: stats.nodes,
          transactions: stats.txs,
          failed: stats.failed,
          counterparties: stats.counterparties,
          first_block: stats.firstBlock,
          last_block: stats.lastBlock,
          flows: stats.flows.map((flow) => ({
            asset: flow.key,
            symbol: flow.symbol,
            decimals: flow.decimals,
            inflow: amount(flow.inflow),
            outflow: amount(flow.outflow),
            internal: amount(flow.internal),
            volume: amount(flow.volume),
          })),
        },
      };
    }),
    hidden: { nodes: [...hiddenNodes], transactions: [...hiddenTxs] },
  };
}

export function downloadJson(data: unknown, filename: string) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
