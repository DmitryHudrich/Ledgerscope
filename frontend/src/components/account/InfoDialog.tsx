import { useCallback, useEffect, useState } from 'react';

import { fetchUsage, type Usage } from '../../api/info';
import { formatBytes, formatCount, formatRelative, splitBytes } from '../../lib/format';
import { FormError, Modal, errorText } from '../Modal';
import { Button, MetaList, PanelTitle, Stat, StatGrid } from '../ui';

interface Props {
  onClose: () => void;
}

function BytesStat({ label, bytes }: { label: string; bytes: number }) {
  const { value, unit } = splitBytes(bytes);
  return <Stat label={label} value={value} unit={unit} />;
}

export function InfoDialog({ onClose }: Props) {
  const [usage, setUsage] = useState<Usage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    setBusy(true);
    setError(null);
    try {
      setUsage(await fetchUsage(signal));
    } catch (cause) {
      if (!signal?.aborted) setError(errorText(cause));
    } finally {
      if (!signal?.aborted) setBusy(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  const storage = usage?.storage;
  const traffic = usage?.traffic;
  const since = usage ? formatRelative(Date.parse(usage.since) / 1000) : null;

  return (
    <Modal
      title="Info"
      subtitle={since ? `Server up since ${since}` : undefined}
      onClose={onClose}
      footer={<Button disabled={busy} onClick={() => void load()}>Refresh</Button>}
    >
      <div className="grid gap-5">
        <section className="grid gap-2">
          <PanelTitle className="mb-0">Stored transactions</PanelTitle>
          {usage && !storage && (
            <p className="m-0 text-xs text-text-secondary">Nothing is stored: transaction storage is off or ClickHouse did not answer.</p>
          )}
          {storage && (
            <>
              <StatGrid>
                <BytesStat label="On disk" bytes={storage.bytes_on_disk} />
                <BytesStat label="Uncompressed" bytes={storage.uncompressed_bytes} />
              </StatGrid>
              <MetaList className="mt-1">
                {storage.chains.map((chain) => (
                  <div key={chain.chain_id}>
                    <dt>{chain.name ?? `Chain ${chain.chain_id}`}</dt>
                    <dd>{formatCount(chain.txs)} txs · ≈{formatBytes(chain.estimated_bytes)}</dd>
                  </div>
                ))}
                {storage.tables.filter((table) => table.table !== 'txs').map((table) => (
                  <div key={table.table}>
                    <dt>{table.table}</dt>
                    <dd>{formatCount(table.rows)} rows · {formatBytes(table.bytes_on_disk)}</dd>
                  </div>
                ))}
              </MetaList>
            </>
          )}
        </section>

        {traffic && (
          <section className="grid gap-2 border-t border-hairline pt-4">
            <PanelTitle className="mb-0">RPC traffic</PanelTitle>
            <StatGrid>
              <BytesStat label="Received" bytes={traffic.received_bytes} />
              <BytesStat label="Sent" bytes={traffic.sent_bytes} />
              <Stat label="Requests" value={formatCount(traffic.calls)} />
              <BytesStat label="Total" bytes={traffic.received_bytes + traffic.sent_bytes} />
            </StatGrid>
            {traffic.chains.length === 0 ? (
              <p className="m-0 text-xs text-text-secondary">No rpc requests since the server started.</p>
            ) : (
              <MetaList className="mt-1">
                {traffic.chains.map((chain) => (
                  <div key={chain.chain_id}>
                    <dt>{chain.name}</dt>
                    <dd>↓ {formatBytes(chain.received_bytes)} · ↑ {formatBytes(chain.sent_bytes)} · {formatCount(chain.calls)} req</dd>
                  </div>
                ))}
              </MetaList>
            )}
            <p className="m-0 text-[11px] text-text-muted">Request and response bodies to rpc nodes, retries included. Counted since the server started.</p>
          </section>
        )}

        <FormError message={error} />
      </div>
    </Modal>
  );
}
