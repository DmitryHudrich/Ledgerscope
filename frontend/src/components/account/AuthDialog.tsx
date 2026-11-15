import { useState, type FormEvent } from 'react';

import { accountApi, type Me } from '../../api/account';
import { FormError, Modal, errorText } from '../Modal';
import { Button, Field, FieldLabel, Input, cx, focusRing } from '../ui';

interface Props {
  registrationOpen: boolean;
  onSignedIn: (me: Me) => void;
  onClose: () => void;
}

type Mode = 'login' | 'register';

export function AuthDialog({ registrationOpen, onSignedIn, onClose }: Props) {
  const [mode, setMode] = useState<Mode>('login');
  const [email, setEmail] = useState('');
  const [name, setName] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const me = mode === 'login'
        ? await accountApi.login(email, password)
        : await accountApi.register(email, name, password);
      onSignedIn(me);
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  const tab = (value: Mode, label: string) => (
    <button
      type="button"
      role="tab"
      aria-selected={mode === value}
      className={cx(
        'h-8 flex-1 cursor-pointer rounded-[5px] text-[13px] font-medium [transition:background_120ms_ease,color_120ms_ease]',
        mode === value ? 'bg-surface-2 text-text-primary shadow-[0_1px_2px_rgba(0,0,0,0.12)]' : 'text-text-muted hover:text-text-primary',
        focusRing,
      )}
      onClick={() => {
        setMode(value);
        setError(null);
      }}
    >
      {label}
    </button>
  );

  return (
    <Modal title={mode === 'login' ? 'Sign in' : 'Create an account'} subtitle="Save canvases, share them with your team and keep your own labels." onClose={onClose} width="sm">
      <form className="grid gap-3" onSubmit={submit}>
        {registrationOpen && (
          <div className="flex rounded-ui-sm border border-hairline bg-plane p-0.5" role="tablist">
            {tab('login', 'Sign in')}
            {tab('register', 'Register')}
          </div>
        )}
        <Field>
          <FieldLabel htmlFor="auth-email">Email</FieldLabel>
          <Input id="auth-email" type="email" autoComplete="email" required value={email} onChange={(event) => setEmail(event.target.value)} />
        </Field>
        {mode === 'register' && (
          <Field>
            <FieldLabel htmlFor="auth-name">Name</FieldLabel>
            <Input id="auth-name" autoComplete="name" required maxLength={64} value={name} onChange={(event) => setName(event.target.value)} />
          </Field>
        )}
        <Field>
          <FieldLabel htmlFor="auth-password">Password</FieldLabel>
          <Input
            id="auth-password"
            type="password"
            autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
            required
            minLength={mode === 'register' ? 10 : undefined}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
          {mode === 'register' && <span className="text-[11px] text-text-muted">At least 10 characters.</span>}
        </Field>
        <FormError message={error} />
        <Button variant="primary" type="submit" className="h-9" disabled={busy}>
          {busy ? 'Please wait…' : mode === 'login' ? 'Sign in' : 'Create account'}
        </Button>
        {!registrationOpen && <p className="m-0 text-center text-[11px] text-text-muted">Registration is closed on this server. Ask an admin for an account.</p>}
      </form>
    </Modal>
  );
}
