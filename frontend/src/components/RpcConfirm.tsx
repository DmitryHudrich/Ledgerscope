import type { RpcConfirmation } from '../api/types';
import { formatCount } from '../lib/format';

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
    <div className="state" role="dialog" aria-modal="true" aria-label="Ask the node?">
      <div className="panel state-card">
        <h2>These blocks are not stored yet</h2>
        <p>
          <strong>{formatCount(confirmation.missing_blocks)}</strong> of{' '}
          {formatCount(confirmation.span.block_count)} blocks are missing from clickhouse —{' '}
          {formatCount(confirmation.indexed_blocks)} are already there. Walking the rest over
          JSON-RPC takes a while and stores them for next time.
        </p>

        <ul className="missing-list">
          {shown.map((range) => (
            <li key={range.from_block}>
              {formatCount(range.from_block)} – {formatCount(range.to_block)}
              <span className="missing-count">{formatCount(range.block_count)} blocks</span>
            </li>
          ))}
          {rest > 0 && <li className="missing-rest">and {formatCount(rest)} more gaps</li>}
        </ul>

        <div className="query">
          <button type="button" className="btn btn-primary" onClick={onConfirm}>
            Ask the node
          </button>
          <button type="button" className="btn" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
