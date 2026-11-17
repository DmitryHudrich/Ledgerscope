import type { RpcConfirmation } from '../api/types';
import { formatCount } from '../lib/format';
import { Button, Panel } from './ui';
import { BlockSpanText } from './BlockSpanText';

const RANGE_PREVIEW = 6;

interface Props {
  confirmation: RpcConfirmation;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RpcConfirm({ confirmation, onConfirm, onCancel }: Props) {
  const shown = confirmation.missing.slice(0, RANGE_PREVIEW);
  const rpcBlocks = Math.max(confirmation.missing_blocks - confirmation.dataset_blocks, 0);
  const rest = confirmation.missing.length - shown.length;

  return (
    <div
      className="absolute inset-0 z-30 grid place-content-center bg-[color-mix(in_srgb,var(--plane)_72%,transparent)] p-6 backdrop-blur-[2px]"
      role="dialog"
      aria-modal="true"
      aria-label="Ask the node?"
    >
      <Panel className="w-full max-w-[440px] p-5 shadow-pop">
        <h2 className="m-0 text-base font-semibold">
          {confirmation.missing_blocks > 0 ? 'These blocks are not stored yet' : 'Fetch exact values again?'}
        </h2>
        <p className="mb-0 mt-2 text-[13px] leading-relaxed text-text-secondary">
          {confirmation.missing_blocks > 0 && (
            <>
              <strong>{formatCount(confirmation.missing_blocks)}</strong> of{' '}
              {formatCount(confirmation.span.block_count)} blocks are missing from ClickHouse,{' '}
              {formatCount(confirmation.indexed_blocks)} are already indexed.{' '}
            </>
          )}
          Fetching takes a while and stores the blocks for the next request.
        </p>
        <ul className="m-0 mt-2 grid list-none gap-0.5 p-0 text-xs text-text-secondary">
          {rpcBlocks > 0 && <li>Over JSON-RPC: <strong className="tabular-nums">{formatCount(rpcBlocks)}</strong> blocks</li>}
          {confirmation.dataset_blocks > 0 && (
            <li>
              From the AWS dataset: <strong className="tabular-nums">{formatCount(confirmation.dataset_blocks)}</strong> blocks,
              {' '}ETH amounts approximate, about 1.3 GB per day to download
            </li>
          )}
          {confirmation.exact_blocks > 0 && (
            <li>Again over JSON-RPC for exact wei: <strong className="tabular-nums">{formatCount(confirmation.exact_blocks)}</strong> blocks</li>
          )}
        </ul>

        <ul className="my-4 grid list-none gap-1 p-0 text-xs text-text-secondary">
          {shown.map((range) => (
            <li className="flex justify-between gap-4 rounded-ui-sm bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)] px-2 py-1.5" key={range.from_block}>
              <BlockSpanText from={range.from_block} to={range.to_block} />
              <span className="text-text-muted">{formatCount(range.block_count)} blocks</span>
            </li>
          ))}
          {rest > 0 && <li className="px-2 py-1 text-text-muted">and {formatCount(rest)} more gaps</li>}
        </ul>

        <div className="flex justify-end gap-2">
          <Button onClick={onCancel}>Cancel</Button>
          <Button variant="primary" onClick={onConfirm}>Fetch</Button>
        </div>
      </Panel>
    </div>
  );
}
