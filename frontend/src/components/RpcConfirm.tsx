import type { RpcConfirmation } from '../api/types';
import { formatCount } from '../lib/format';
import { Button, Panel } from './ui';

const RANGE_PREVIEW = 6;

interface Props {
  confirmation: RpcConfirmation;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RpcConfirm({ confirmation, onConfirm, onCancel }: Props) {
  const shown = confirmation.missing.slice(0, RANGE_PREVIEW);
  const rest = confirmation.missing.length - shown.length;

  return (
    <div
      className="absolute inset-0 z-30 grid place-content-center bg-[color-mix(in_srgb,var(--plane)_72%,transparent)] p-6 backdrop-blur-[2px]"
      role="dialog"
      aria-modal="true"
      aria-label="Ask the node?"
    >
      <Panel className="w-full max-w-[440px] p-5 shadow-pop">
        <h2 className="m-0 text-base font-semibold">These blocks are not stored yet</h2>
        <p className="mb-0 mt-2 text-[13px] leading-relaxed text-text-secondary">
          <strong>{formatCount(confirmation.missing_blocks)}</strong> of{' '}
          {formatCount(confirmation.span.block_count)} blocks are missing from ClickHouse.{' '}
          {formatCount(confirmation.indexed_blocks)} are already indexed. Fetching the rest over
          JSON-RPC can take a while and stores them for the next request.
        </p>

        <ul className="my-4 grid list-none gap-1 p-0 text-xs text-text-secondary">
          {shown.map((range) => (
            <li className="flex justify-between gap-4 rounded-ui-sm bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)] px-2 py-1.5" key={range.from_block}>
              <span className="tabular-nums">{formatCount(range.from_block)} – {formatCount(range.to_block)}</span>
              <span className="text-text-muted">{formatCount(range.block_count)} blocks</span>
            </li>
          ))}
          {rest > 0 && <li className="px-2 py-1 text-text-muted">and {formatCount(rest)} more gaps</li>}
        </ul>

        <div className="flex justify-end gap-2">
          <Button onClick={onCancel}>Cancel</Button>
          <Button variant="primary" onClick={onConfirm}>Ask the node</Button>
        </div>
      </Panel>
    </div>
  );
}
