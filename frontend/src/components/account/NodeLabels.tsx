import { useCallback, useEffect, useState, type FormEvent } from 'react';

import { canEditTeam, labelApi, ownerName, type Me, type UserLabel } from '../../api/account';
import { errorText } from '../Modal';
import { Button, Input, PanelSection, PanelTitle, cx, focusRing } from '../ui';

interface Props {
  me: Me | null;
  chainId: number | null;
  address: string;
  readOnly: boolean;
  onSignIn: () => void;
  onChanged: (address: string, labels: UserLabel[]) => void;
}

export function NodeLabels({ me, chainId, address, readOnly, onSignIn, onChanged }: Props) {
  const [labels, setLabels] = useState<UserLabel[]>([]);
  const [value, setValue] = useState('');
  const [note, setNote] = useState('');
  const [owner, setOwner] = useState('user');
  const [editing, setEditing] = useState<string | null>(null);
  const [editValue, setEditValue] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    if (!me || chainId === null) {
      setLabels([]);
      return [];
    }
    const next = await labelApi.list(chainId, [address]);
    setLabels(next);
    return next;
  }, [address, chainId, me]);

  useEffect(() => {
    setError(null);
    setEditing(null);
    reload().catch((cause) => setError(errorText(cause)));
  }, [reload]);

  const attempt = async (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await work();
      onChanged(address, await reload());
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  if (readOnly) return null;

  if (!me) {
    return (
      <PanelSection>
        <PanelTitle>Your labels</PanelTitle>
        <p className="m-0 text-xs text-text-secondary">
          <button type="button" className={cx('cursor-pointer text-accent underline-offset-2 hover:underline', focusRing)} onClick={onSignIn}>Sign in</button>{' '}
          to name this address for yourself or your team.
        </p>
      </PanelSection>
    );
  }

  const teams = me.teams.filter((team) => canEditTeam(team.role));

  const add = (event: FormEvent) => {
    event.preventDefault();
    if (chainId === null) return;
    void attempt(async () => {
      await labelApi.create(chainId, address, value, note, owner === 'user' ? { kind: 'user' } : { kind: 'team', team_id: owner });
      setValue('');
      setNote('');
    });
  };

  return (
    <PanelSection>
      <PanelTitle>Your labels</PanelTitle>
      <div className="grid gap-1.5">
        {labels.map((label) => (
          <div key={label.id} className="group grid gap-0.5 rounded-ui-sm border border-hairline px-2 py-1.5">
            {editing === label.id ? (
              <form
                className="flex gap-1.5"
                onSubmit={(event) => {
                  event.preventDefault();
                  void attempt(async () => {
                    await labelApi.update(label.id, editValue, label.note);
                    setEditing(null);
                  });
                }}
              >
                <Input className="h-7 min-w-0 flex-1 text-xs" maxLength={64} value={editValue} autoFocus onChange={(event) => setEditValue(event.target.value)} />
                <Button type="submit" className="h-7 px-2 text-xs" disabled={busy}>Save</Button>
              </form>
            ) : (
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{label.value}</span>
                <span className="text-[10px] uppercase tracking-[0.06em] text-text-muted">{ownerName(label.owner)}</span>
                {label.writable && (
                  <span className="flex gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100">
                    <Button variant="ghost" className="h-6 px-1.5 text-[11px]" onClick={() => { setEditing(label.id); setEditValue(label.value); }}>Edit</Button>
                    <Button variant="ghost" className="h-6 px-1.5 text-[11px] text-critical" disabled={busy} onClick={() => void attempt(() => labelApi.delete(label.id))}>Delete</Button>
                  </span>
                )}
              </div>
            )}
            {label.note && editing !== label.id && <span className="text-[11px] text-text-secondary">{label.note}</span>}
          </div>
        ))}
        <form className="grid gap-1.5" onSubmit={add}>
          <div className="flex gap-1.5">
            <Input className="h-8 min-w-0 flex-1 text-xs" placeholder="Add a label" maxLength={64} value={value} onChange={(event) => setValue(event.target.value)} />
            <Button type="submit" className="h-8" disabled={busy || !value.trim() || chainId === null}>Add</Button>
          </div>
          {value.trim() && (
            <>
              <Input className="h-8 text-xs" placeholder="Note, optional" maxLength={500} value={note} onChange={(event) => setNote(event.target.value)} />
              {teams.length > 0 && (
                <select
                  className={cx('h-8 cursor-pointer rounded-ui-sm border border-hairline bg-plane px-2 text-xs hover:border-hairline-strong', focusRing)}
                  value={owner}
                  aria-label="Who sees this label"
                  onChange={(event) => setOwner(event.target.value)}
                >
                  <option value="user">Only me</option>
                  {teams.map((team) => <option key={team.id} value={team.id}>Team · {team.name}</option>)}
                </select>
              )}
            </>
          )}
        </form>
        {error && <p className="m-0 text-[11px] text-critical" role="alert">{error}</p>}
      </div>
    </PanelSection>
  );
}
