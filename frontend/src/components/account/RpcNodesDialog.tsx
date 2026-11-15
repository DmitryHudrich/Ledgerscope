import { useEffect, useState } from 'react';

import type { ChainInfo } from '../../api/types';
import { rpcNodeApi, type PersonalRpcNode } from '../../api/account';
import { formatRelative } from '../../lib/format';
import { FormError, Modal, errorText } from '../Modal';
import { Button, Input, cx, focusRing } from '../ui';

interface Props {
  chains: ChainInfo[];
  chainId: number | null;
  onClose: () => void;
}

const selectClass = cx(
  'h-[30px] max-w-[180px] cursor-pointer rounded-ui-sm border border-hairline bg-plane px-2 text-xs text-inherit hover:border-hairline-strong',
  focusRing,
);

export function RpcNodesDialog({ chains, chainId, onClose }: Props) {
  const [nodes, setNodes] = useState<PersonalRpcNode[] | null>(null);
  const [chain, setChain] = useState<number>(chainId ?? chains[0]?.chain_id ?? 1);
  const [url, setUrl] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = async () => {
    try {
      setNodes(await rpcNodeApi.list());
    } catch (cause) {
      setError(errorText(cause));
      setNodes([]);
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  const chainName = (id: number) => chains.find((candidate) => candidate.chain_id === id)?.name ?? `Chain ${id}`;

  const add = async () => {
    setBusy(true);
    setError(null);
    try {
      await rpcNodeApi.add(chain, url.trim());
      setUrl('');
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (node: PersonalRpcNode) => {
    setError(null);
    try {
      await rpcNodeApi.delete(node.id);
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  return (
    <Modal
      title="My RPC nodes"
      subtitle="Your own nodes on top of the server’s configured and discovered ones. Your graph requests ask them first and fall back to the shared pool. Nobody else uses them."
      onClose={onClose}
      width="lg"
    >
      <div className="grid gap-3">
        <form
          className="flex flex-wrap items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void add();
          }}
        >
          <select className={selectClass} value={chain} aria-label="Chain" onChange={(event) => setChain(Number(event.target.value))}>
            {chains.map((candidate) => (
              <option key={candidate.chain_id} value={candidate.chain_id}>{candidate.name}</option>
            ))}
          </select>
          <Input
            className="min-w-[220px] flex-1 font-mono-ui text-xs"
            placeholder="https://mainnet.example.io/v3/your-key"
            value={url}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => setUrl(event.target.value)}
          />
          <Button type="submit" variant="primary" disabled={busy || !url.trim()}>
            {busy ? 'Checking…' : 'Add'}
          </Button>
        </form>
        <p className="m-0 text-[11px] text-text-muted">
          The node is checked before it is saved: it must answer as the chosen chain. Keys in the URL are kept on the server and never shown again.
        </p>
        <FormError message={error} />
        {nodes === null && <p className="m-0 text-[13px] text-text-muted">Loading…</p>}
        {nodes !== null && nodes.length === 0 && (
          <p className="m-0 py-6 text-center text-[13px] text-text-muted">No nodes of your own yet.</p>
        )}
        <div className="grid gap-1">
          {nodes?.map((node) => (
            <div key={node.id} className="flex items-center gap-3 rounded-ui-sm px-2.5 py-2 hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]">
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono-ui text-xs font-medium">{node.label}</div>
                <div className="text-[11px] text-text-muted">
                  {chainName(node.chain_id)} · added {formatRelative(Date.parse(node.created_at) / 1000)}
                </div>
              </div>
              <Button variant="ghost" className="h-7 px-2 text-xs text-critical" onClick={() => void remove(node)}>Delete</Button>
            </div>
          ))}
        </div>
      </div>
    </Modal>
  );
}
