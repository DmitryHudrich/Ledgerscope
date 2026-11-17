import { useState, type FormEvent } from 'react';

import { FormError, Modal } from '../Modal';
import { Button, Field, FieldLabel, Input } from '../ui';

interface Props {
  canvasTitle: string;
  busy: boolean;
  error: string | null;
  onSave: (label: string) => void;
  onClose: () => void;
}

export function SnapshotDialog({ canvasTitle, busy, error, onSave, onClose }: Props) {
  const [label, setLabel] = useState('');

  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSave(label.trim());
  };

  return (
    <Modal title="Save snapshot" subtitle={`A named point in “${canvasTitle}” you can come back to from the canvas list.`} onClose={onClose} width="sm">
      <form className="grid gap-3" onSubmit={submit}>
        <Field>
          <FieldLabel htmlFor="snapshot-label">Name</FieldLabel>
          <Input id="snapshot-label" maxLength={120} autoFocus placeholder="Before following the bridge" value={label} onChange={(event) => setLabel(event.target.value)} />
        </Field>
        <FormError message={error} />
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" type="submit" disabled={busy}>{busy ? 'Saving…' : 'Save snapshot'}</Button>
        </div>
      </form>
    </Modal>
  );
}
