import { useEffect, useMemo, useState } from 'react';
import { CopyAddress } from '../CopyAddress';

import type { ChainInfo } from '../../api/types';
import { labelApi, ownerName, type UserLabel } from '../../api/account';
import { displayAddress } from '../../lib/chain';
import { formatRelative, shortAddress } from '../../lib/format';
import { FormError, Modal, errorText } from '../Modal';
import { IconSearch } from '../Icons';
import { Button, Input, cx, focusRing } from '../ui';

interface Props {
  chains: ChainInfo[];
  chainId: number | null;
  onFocusAddress: (address: string) => void;
  onChanged: (address: string) => void;
  onClose: () => void;
}

export function LabelsDialog({ chains, chainId, onFocusAddress, onChanged, onClose }: Props) {
  const [labels, setLabels] = useState<UserLabel[] | null>(null);
  const [onlyChain, setOnlyChain] = useState(chainId !== null);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<string | null>(null);
  const [value, setValue] = useState('');
  const [note, setNote] = useState('');
  const [error, setError] = useState<string | null>(null);

  const reload = async () => {
    try {
      setLabels(await labelApi.list(onlyChain && chainId !== null ? chainId : undefined));
    } catch (cause) {
      setError(errorText(cause));
      setLabels([]);
    }
  };

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onlyChain, chainId]);

  const needle = query.trim().toLowerCase();
  const shown = useMemo(
    () => (labels ?? []).filter((label) =>
      !needle
      || label.value.toLowerCase().includes(needle)
      || label.note.toLowerCase().includes(needle)
      || label.address.toLowerCase().includes(needle)
      || displayAddress(label.address).toLowerCase().includes(needle)),
    [labels, needle],
  );

  const chainName = (id: number) => chains.find((chain) => chain.chain_id === id)?.name ?? `Chain ${id}`;

  const save = async (label: UserLabel) => {
    try {
      await labelApi.update(label.id, value, note);
      setEditing(null);
      onChanged(label.address);
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  const remove = async (label: UserLabel) => {
    try {
      await labelApi.delete(label.id);
      onChanged(label.address);
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    }
  };

  return (
    <Modal title="My labels" subtitle="Your own labels and the ones of your teams. They show on the graph before the server’s." onClose={onClose} width="lg">
      <div className="grid gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <label className="flex h-8 min-w-[200px] flex-1 items-center gap-2 rounded-ui-sm border border-hairline bg-plane px-2.5 text-text-muted focus-within:border-accent">
            <IconSearch size={14} />
            <input
              className="h-full min-w-0 flex-1 bg-transparent text-[13px] text-text-primary outline-none placeholder:text-text-muted"
              placeholder="Search label, note or address"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          {chainId !== null && (
            <label className="flex items-center gap-1.5 text-xs text-text-secondary">
              <input type="checkbox" className="accent-accent" checked={onlyChain} onChange={(event) => setOnlyChain(event.target.checked)} />
              Only {chainName(chainId)}
            </label>
          )}
        </div>
        <FormError message={error} />
        {labels === null && <p className="m-0 text-[13px] text-text-muted">Loading…</p>}
        {labels !== null && shown.length === 0 && (
          <p className="m-0 py-6 text-center text-[13px] text-text-muted">
            {needle ? 'Nothing matches.' : 'No labels yet. Select an address on the graph to add one.'}
          </p>
        )}
        <div className="grid gap-1">
          {shown.map((label) => (
            <div key={label.id} className="grid gap-1 rounded-ui-sm px-2.5 py-2 hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]">
              {editing === label.id ? (
                <form
                  className="grid gap-1.5"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void save(label);
                  }}
                >
                  <Input className="h-8 text-[13px]" maxLength={64} value={value} autoFocus onChange={(event) => setValue(event.target.value)} />
                  <Input className="h-8 text-xs" maxLength={500} placeholder="Note" value={note} onChange={(event) => setNote(event.target.value)} />
                  <div className="flex gap-2">
                    <Button type="submit" variant="primary" className="h-7">Save</Button>
                    <Button className="h-7" onClick={() => setEditing(null)}>Cancel</Button>
                  </div>
                </form>
              ) : (
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px] font-medium">{label.value}</div>
                    <button
                      type="button"
                      className={cx('cursor-pointer font-mono-ui text-[11px] text-text-muted hover:text-accent', focusRing)}
                      title={displayAddress(label.address)}
                      onClick={() => onFocusAddress(label.address)}
                    >
                      {shortAddress(displayAddress(label.address), 8, 6)}
                    </button>
                    <CopyAddress address={label.address} className="ml-0.5" />
                    {label.note && <div className="text-[11px] text-text-secondary">{label.note}</div>}
                  </div>
                  <div className="grid justify-items-end text-[11px] text-text-muted">
                    <span>{ownerName(label.owner)}</span>
                    <span>{chainName(label.chain_id)} · {formatRelative(Date.parse(label.updated_at) / 1000)}</span>
                  </div>
                  {label.writable && (
                    <div className="flex gap-1">
                      <Button variant="ghost" className="h-7 px-2 text-xs" onClick={() => { setEditing(label.id); setValue(label.value); setNote(label.note); }}>Edit</Button>
                      <Button variant="ghost" className="h-7 px-2 text-xs text-critical" onClick={() => void remove(label)}>Delete</Button>
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
    </Modal>
  );
}
