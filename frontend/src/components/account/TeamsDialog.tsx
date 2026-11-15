import { useCallback, useEffect, useState, type FormEvent } from 'react';

import {
  ROLE_TITLE,
  canManageTeam,
  teamApi,
  type Me,
  type TeamDetail,
  type TeamRole,
} from '../../api/account';
import { FormError, Modal, errorText } from '../Modal';
import { Badge, Button, Input, PanelTitle, cx, focusRing } from '../ui';

interface Props {
  me: Me;
  onTeamsChanged: () => Promise<void>;
  onClose: () => void;
}

const ROLES: TeamRole[] = ['owner', 'admin', 'editor', 'viewer'];
const ROLE_HINT: Record<TeamRole, string> = {
  owner: 'Everything, including deleting the team',
  admin: 'Manages members and every team canvas',
  editor: 'Creates and edits team canvases and labels',
  viewer: 'Reads team canvases and labels',
};

function grantable(mine: TeamRole): TeamRole[] {
  if (mine === 'owner') return ROLES;
  if (mine === 'admin') return ['admin', 'editor', 'viewer'];
  return [];
}

const selectClass = cx(
  'h-[30px] cursor-pointer rounded-ui-sm border border-hairline bg-plane px-2 text-xs text-inherit hover:border-hairline-strong',
  focusRing,
);

export function TeamsDialog({ me, onTeamsChanged, onClose }: Props) {
  const [selected, setSelected] = useState<string | null>(me.teams[0]?.id ?? null);
  const [detail, setDetail] = useState<TeamDetail | null>(null);
  const [newTeam, setNewTeam] = useState('');
  const [email, setEmail] = useState('');
  const [role, setRole] = useState<TeamRole>('editor');
  const [rename, setRename] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async (id: string | null) => {
    if (!id) {
      setDetail(null);
      return;
    }
    try {
      const next = await teamApi.detail(id);
      setDetail(next);
      setRename(next.team.name);
    } catch (cause) {
      setError(errorText(cause));
    }
  }, []);

  useEffect(() => {
    void load(selected);
  }, [load, selected]);

  const attempt = async (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  const create = (event: FormEvent) => {
    event.preventDefault();
    void attempt(async () => {
      const team = await teamApi.create(newTeam);
      setNewTeam('');
      await onTeamsChanged();
      setSelected(team.id);
    });
  };

  const add = (event: FormEvent) => {
    event.preventDefault();
    if (!detail) return;
    void attempt(async () => {
      await teamApi.addMember(detail.team.id, email, role);
      setEmail('');
      await load(detail.team.id);
      await onTeamsChanged();
    });
  };

  const mine = detail?.team.role ?? 'viewer';
  const manage = canManageTeam(mine);
  const offered = grantable(mine);

  return (
    <Modal title="Teams" subtitle="Team canvases and labels are shared by every member, as far as their role allows." onClose={onClose} width="lg">
      <div className="grid gap-4 md:grid-cols-[200px_1fr]">
        <aside className="grid content-start gap-2">
          <div className="grid gap-1">
            {me.teams.map((team) => (
              <button
                key={team.id}
                type="button"
                className={cx(
                  'flex cursor-pointer items-center justify-between gap-2 rounded-ui-sm px-2.5 py-2 text-left text-[13px]',
                  selected === team.id ? 'bg-[color-mix(in_srgb,var(--accent)_14%,transparent)] text-accent' : 'hover:bg-[color-mix(in_srgb,var(--text-primary)_6%,transparent)]',
                  focusRing,
                )}
                onClick={() => setSelected(team.id)}
              >
                <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">{team.name}</span>
                <Badge>{team.members}</Badge>
              </button>
            ))}
            {me.teams.length === 0 && <p className="m-0 text-xs text-text-muted">You are in no team yet.</p>}
          </div>
          <form className="grid gap-1.5 border-t border-hairline pt-3" onSubmit={create}>
            <PanelTitle className="mb-0">New team</PanelTitle>
            <Input placeholder="Team name" maxLength={64} value={newTeam} onChange={(event) => setNewTeam(event.target.value)} />
            <Button type="submit" disabled={busy || !newTeam.trim()}>Create</Button>
          </form>
        </aside>

        <section className="grid min-w-0 content-start gap-4">
          {!detail && <p className="m-0 text-[13px] text-text-muted">Pick or create a team.</p>}
          {detail && (
            <>
              <div className="flex flex-wrap items-center gap-2">
                {manage ? (
                  <form
                    className="flex min-w-0 flex-1 gap-2"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void attempt(async () => {
                        await teamApi.rename(detail.team.id, rename);
                        await load(detail.team.id);
                        await onTeamsChanged();
                      });
                    }}
                  >
                    <Input className="min-w-0 flex-1 font-semibold" maxLength={64} value={rename} onChange={(event) => setRename(event.target.value)} />
                    <Button type="submit" disabled={busy || rename.trim() === detail.team.name || !rename.trim()}>Rename</Button>
                  </form>
                ) : (
                  <h3 className="m-0 flex-1 text-[15px] font-semibold">{detail.team.name}</h3>
                )}
                <Badge>You are {ROLE_TITLE[mine].toLowerCase()}</Badge>
              </div>

              <div className="grid gap-1">
                <PanelTitle className="mb-1">Members · {detail.members.length}</PanelTitle>
                {detail.members.map((member) => {
                  const self = member.user.id === me.user.id;
                  const editable = manage && offered.includes(member.role) && !self;
                  return (
                    <div key={member.user.id} className="flex items-center gap-2 rounded-ui-sm px-2 py-1.5 hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]">
                      <div className="min-w-0 flex-1">
                        <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[13px]">
                          {member.user.name}
                          {self && <span className="ml-1.5 text-[11px] text-text-muted">you</span>}
                        </div>
                        <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-text-muted">{member.user.email}</div>
                      </div>
                      {editable ? (
                        <select
                          className={selectClass}
                          value={member.role}
                          aria-label={`Role of ${member.user.name}`}
                          disabled={busy}
                          onChange={(event) => void attempt(async () => {
                            await teamApi.changeRole(detail.team.id, member.user.id, event.target.value as TeamRole);
                            await load(detail.team.id);
                          })}
                        >
                          {offered.map((option) => <option key={option} value={option}>{ROLE_TITLE[option]}</option>)}
                        </select>
                      ) : (
                        <span className="text-xs text-text-secondary">{ROLE_TITLE[member.role]}</span>
                      )}
                      {(editable || self) && (
                        <Button
                          variant="ghost"
                          className="h-7 px-2 text-xs"
                          disabled={busy}
                          onClick={() => void attempt(async () => {
                            await teamApi.removeMember(detail.team.id, member.user.id);
                            await onTeamsChanged();
                            if (self) {
                              setSelected(null);
                            } else {
                              await load(detail.team.id);
                            }
                          })}
                        >
                          {self ? 'Leave' : 'Remove'}
                        </Button>
                      )}
                    </div>
                  );
                })}
              </div>

              {manage && (
                <form className="grid gap-1.5 border-t border-hairline pt-3" onSubmit={add}>
                  <PanelTitle className="mb-0">Add a member</PanelTitle>
                  <div className="flex flex-wrap gap-2">
                    <Input className="min-w-[180px] flex-1" type="email" placeholder="Their account email" value={email} onChange={(event) => setEmail(event.target.value)} />
                    <select className={selectClass} value={role} aria-label="Role" onChange={(event) => setRole(event.target.value as TeamRole)}>
                      {offered.map((option) => <option key={option} value={option}>{ROLE_TITLE[option]}</option>)}
                    </select>
                    <Button type="submit" variant="primary" disabled={busy || !email.trim()}>Add</Button>
                  </div>
                  <span className="text-[11px] text-text-muted">{ROLE_HINT[role]}. They need an account first.</span>
                </form>
              )}

              {mine === 'owner' && (
                <div className="flex items-center justify-between gap-2 border-t border-hairline pt-3">
                  <span className="text-xs text-text-muted">Deleting the team removes its canvases and labels.</span>
                  <Button
                    className="text-critical"
                    disabled={busy}
                    onClick={() => {
                      if (!window.confirm(`Delete ${detail.team.name} with its canvases and labels?`)) return;
                      void attempt(async () => {
                        await teamApi.delete(detail.team.id);
                        setSelected(null);
                        await onTeamsChanged();
                      });
                    }}
                  >
                    Delete team
                  </Button>
                </div>
              )}
            </>
          )}
          <FormError message={error} />
        </section>
      </div>
    </Modal>
  );
}
