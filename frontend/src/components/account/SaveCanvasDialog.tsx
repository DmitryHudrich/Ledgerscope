import { useState, type FormEvent } from 'react';

import { canEditTeam, type Me, type OwnerRequest } from '../../api/account';
import { FormError, Modal } from '../Modal';
import { Button, Field, FieldLabel, Input, cx, focusRing } from '../ui';

interface Props {
  me: Me;
  initialTitle: string;
  busy: boolean;
  error: string | null;
  onSave: (title: string, owner: OwnerRequest) => void;
  onClose: () => void;
}

export function SaveCanvasDialog({ me, initialTitle, busy, error, onSave, onClose }: Props) {
  const [title, setTitle] = useState(initialTitle);
  const [owner, setOwner] = useState('user');
  const teams = me.teams.filter((team) => canEditTeam(team.role));

  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSave(title, owner === 'user' ? { kind: 'user' } : { kind: 'team', team_id: owner });
  };

  return (
    <Modal title="Save canvas" subtitle="Saves the graph as it is now: roots, block range, layout, drawings and filters." onClose={onClose} width="sm">
      <form className="grid gap-3" onSubmit={submit}>
        <Field>
          <FieldLabel htmlFor="canvas-title">Title</FieldLabel>
          <Input id="canvas-title" required maxLength={120} value={title} onChange={(event) => setTitle(event.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor="canvas-owner">Belongs to</FieldLabel>
          <select
            id="canvas-owner"
            className={cx('h-[30px] cursor-pointer rounded-ui-sm border border-hairline bg-plane px-2 text-[13px] hover:border-hairline-strong', focusRing)}
            value={owner}
            onChange={(event) => setOwner(event.target.value)}
          >
            <option value="user">Personal, only you and who you share it with</option>
            {teams.map((team) => (
              <option key={team.id} value={team.id}>Team · {team.name}</option>
            ))}
          </select>
        </Field>
        <FormError message={error} />
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" type="submit" disabled={busy || !title.trim()}>{busy ? 'Saving…' : 'Save'}</Button>
        </div>
      </form>
    </Modal>
  );
}
