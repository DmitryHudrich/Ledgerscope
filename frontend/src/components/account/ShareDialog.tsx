import { useCallback, useEffect, useState, type FormEvent } from 'react';

import { canvasApi, ownerName, type CanvasInfo, type PublicLink, type Share, type ShareAccess } from '../../api/account';
import { formatRelative } from '../../lib/format';
import { FormError, Modal, errorText } from '../Modal';
import { IconCopy } from '../Icons';
import { Badge, Button, Input, PanelTitle, cx, focusRing } from '../ui';

interface Props {
  canvas: CanvasInfo;
  onClose: () => void;
}

const selectClass = cx(
  'h-[30px] cursor-pointer rounded-ui-sm border border-hairline bg-plane px-2 text-xs text-inherit hover:border-hairline-strong',
  focusRing,
);

export function publicUrl(token: string): string {
  const url = new URL(window.location.href);
  url.search = '';
  url.hash = '';
  url.searchParams.set('share', token);
  return url.toString();
}

export function ShareDialog({ canvas, onClose }: Props) {
  const [shares, setShares] = useState<Share[]>([]);
  const [links, setLinks] = useState<PublicLink[]>([]);
  const [email, setEmail] = useState('');
  const [access, setAccess] = useState<ShareAccess>('view');
  const [fresh, setFresh] = useState<{ id: string; url: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      const [nextShares, nextLinks] = await Promise.all([canvasApi.shares(canvas.id), canvasApi.links(canvas.id)]);
      setShares(nextShares);
      setLinks(nextLinks);
    } catch (cause) {
      setError(errorText(cause));
    }
  }, [canvas.id]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const attempt = async (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await work();
      await reload();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  };

  const invite = (event: FormEvent) => {
    event.preventDefault();
    void attempt(async () => {
      await canvasApi.share(canvas.id, email, access);
      setEmail('');
    });
  };

  const copy = async (url: string) => {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  const live = links.filter((link) => !link.revoked_at);

  return (
    <Modal title={`Share “${canvas.title}”`} subtitle={canvas.owner.kind === 'team' ? `Every member of ${ownerName(canvas.owner)} sees it by their role already.` : 'Only you see it until you share it.'} onClose={onClose}>
      <div className="grid gap-5">
        <section className="grid gap-2">
          <PanelTitle className="mb-0">People</PanelTitle>
          <form className="flex flex-wrap gap-2" onSubmit={invite}>
            <Input className="min-w-[180px] flex-1" type="email" placeholder="Their account email" value={email} onChange={(event) => setEmail(event.target.value)} />
            <select className={selectClass} value={access} aria-label="Access" onChange={(event) => setAccess(event.target.value as ShareAccess)}>
              <option value="view">Can view</option>
              <option value="edit">Can edit</option>
            </select>
            <Button type="submit" variant="primary" disabled={busy || !email.trim()}>Share</Button>
          </form>
          {shares.length === 0 && <p className="m-0 text-xs text-text-muted">Not shared with anyone personally.</p>}
          {shares.map((share) => (
            <div key={share.user.id} className="flex items-center gap-2 rounded-ui-sm px-2 py-1.5 hover:bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)]">
              <div className="min-w-0 flex-1">
                <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[13px]">{share.user.name}</div>
                <div className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-text-muted">{share.user.email}</div>
              </div>
              <select
                className={selectClass}
                value={share.access}
                aria-label={`Access of ${share.user.name}`}
                disabled={busy}
                onChange={(event) => void attempt(async () => {
                  await canvasApi.share(canvas.id, share.user.email, event.target.value as ShareAccess);
                })}
              >
                <option value="view">Can view</option>
                <option value="edit">Can edit</option>
              </select>
              <Button variant="ghost" className="h-7 px-2 text-xs" disabled={busy} onClick={() => void attempt(() => canvasApi.unshare(canvas.id, share.user.id))}>
                Remove
              </Button>
            </div>
          ))}
        </section>

        <section className="grid gap-2 border-t border-hairline pt-4">
          <div className="flex items-center justify-between gap-2">
            <PanelTitle className="mb-0">Public read-only links</PanelTitle>
            <Button
              disabled={busy}
              onClick={() => void attempt(async () => {
                const created = await canvasApi.createLink(canvas.id);
                setFresh({ id: created.link.id, url: publicUrl(created.token) });
              })}
            >
              New link
            </Button>
          </div>
          <p className="m-0 text-xs text-text-muted">Anyone with the link opens a read-only copy, no account needed. A link is shown once, revoke it when it is out.</p>
          {fresh && (
            <div className="grid gap-1.5 rounded-ui-sm border border-[color-mix(in_srgb,var(--good)_40%,transparent)] bg-[color-mix(in_srgb,var(--good)_8%,transparent)] p-2.5">
              <span className="text-[11px] text-text-secondary">Copy it now, it will not be shown again.</span>
              <div className="flex gap-2">
                <Input className="min-w-0 flex-1 font-mono-ui text-[11px]" readOnly value={fresh.url} onFocus={(event) => event.target.select()} />
                <Button onClick={() => void copy(fresh.url)}><IconCopy size={13} /> {copied ? 'Copied' : 'Copy'}</Button>
              </div>
            </div>
          )}
          {live.length === 0 && !fresh && <p className="m-0 text-xs text-text-muted">No live links.</p>}
          {live.map((link) => (
            <div key={link.id} className="flex items-center gap-2 px-2 py-1 text-xs">
              <span className="flex-1 text-text-secondary">Created {formatRelative(Date.parse(link.created_at) / 1000)}</span>
              {fresh?.id === link.id && <Badge>new</Badge>}
              <Button
                variant="ghost"
                className="h-7 px-2 text-xs text-critical"
                disabled={busy}
                onClick={() => void attempt(async () => {
                  await canvasApi.revokeLink(canvas.id, link.id);
                  if (fresh?.id === link.id) setFresh(null);
                })}
              >
                Revoke
              </Button>
            </div>
          ))}
        </section>

        <FormError message={error} />
      </div>
    </Modal>
  );
}
