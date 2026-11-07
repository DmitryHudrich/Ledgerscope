import { useState, type FormEvent } from 'react';

import { accountApi, type Me } from '../../api/account';
import { FormError, Modal, errorText } from '../Modal';
import { Button, Field, FieldLabel, Input, PanelTitle } from '../ui';

interface Props {
  me: Me;
  onChanged: (me: Me) => void;
  onSignedOutEverywhere: () => void;
  onClose: () => void;
}

export function ProfileDialog({ me, onChanged, onSignedOutEverywhere, onClose }: Props) {
  const [name, setName] = useState(me.user.name);
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const attempt = async (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await work();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  const rename = (event: FormEvent) => {
    event.preventDefault();
    void attempt(async () => {
      onChanged(await accountApi.rename(name));
      setNotice('Name saved.');
    });
  };

  const changePassword = (event: FormEvent) => {
    event.preventDefault();
    void attempt(async () => {
      onChanged(await accountApi.changePassword(current, next));
      setCurrent('');
      setNext('');
      setNotice('Password changed. Other devices are signed out.');
    });
  };

  return (
    <Modal title="Profile" subtitle={me.user.email} onClose={onClose} width="sm">
      <div className="grid gap-5">
        <form className="grid gap-2" onSubmit={rename}>
          <PanelTitle className="mb-0">Name</PanelTitle>
          <div className="flex gap-2">
            <Input className="flex-1" maxLength={64} value={name} onChange={(event) => setName(event.target.value)} />
            <Button type="submit" disabled={busy || name.trim() === me.user.name}>Save</Button>
          </div>
        </form>

        <form className="grid gap-2" onSubmit={changePassword}>
          <PanelTitle className="mb-0">Password</PanelTitle>
          <Field>
            <FieldLabel htmlFor="pw-current">Current password</FieldLabel>
            <Input id="pw-current" type="password" autoComplete="current-password" value={current} onChange={(event) => setCurrent(event.target.value)} />
          </Field>
          <Field>
            <FieldLabel htmlFor="pw-next">New password</FieldLabel>
            <Input id="pw-next" type="password" autoComplete="new-password" minLength={10} value={next} onChange={(event) => setNext(event.target.value)} />
          </Field>
          <Button type="submit" className="justify-self-start" disabled={busy || !current || next.length < 10}>Change password</Button>
        </form>

        <div className="grid gap-2 border-t border-hairline pt-4">
          <PanelTitle className="mb-0">Sessions</PanelTitle>
          <p className="m-0 text-xs text-text-secondary">Sign out on every device, this one included.</p>
          <Button
            className="justify-self-start"
            disabled={busy}
            onClick={() => void attempt(async () => {
              await accountApi.logoutEverywhere();
              onSignedOutEverywhere();
            })}
          >
            Sign out everywhere
          </Button>
        </div>

        {notice && <p className="m-0 text-xs text-good" role="status">{notice}</p>}
        <FormError message={error} />
      </div>
    </Modal>
  );
}
