import { useState, type FormEvent } from 'react';

import type { GraphGroup } from '../lib/groups';
import { Button, Input, cx, focusRing } from './ui';

interface Props {
  groups: GraphGroup[];
  onPick: (target: string | { name: string }) => void;
  label?: string;
  compact?: boolean;
}

const NEW = '__new__';

export function GroupPicker({ groups, onPick, label = 'Add to group', compact = false }: Props) {
  const [choice, setChoice] = useState('');
  const [name, setName] = useState('');

  const naming = choice === NEW || groups.length === 0;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (naming) {
      if (!name.trim()) return;
      onPick({ name });
      setName('');
      setChoice('');
    } else if (choice) {
      onPick(choice);
      setChoice('');
    }
  };

  return (
    <form className="flex min-w-0 gap-1.5" onSubmit={submit}>
      {groups.length > 0 && (
        <select
          className={cx('h-8 min-w-0 flex-1 cursor-pointer rounded-ui-sm border border-hairline bg-plane px-2 text-xs hover:border-hairline-strong', focusRing, compact && 'h-7')}
          value={choice}
          aria-label={label}
          onChange={(event) => setChoice(event.target.value)}
        >
          <option value="" disabled>{label}…</option>
          {groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}
          <option value={NEW}>New group…</option>
        </select>
      )}
      {naming && (
        <Input
          className={cx('h-8 min-w-0 flex-1 text-xs', compact && 'h-7')}
          placeholder="New group name"
          maxLength={60}
          value={name}
          autoFocus={choice === NEW}
          onChange={(event) => setName(event.target.value)}
        />
      )}
      <Button type="submit" className={cx('h-8 px-2.5 text-xs', compact && 'h-7')} disabled={naming ? !name.trim() : !choice}>
        {naming ? 'Create' : 'Add'}
      </Button>
    </form>
  );
}
