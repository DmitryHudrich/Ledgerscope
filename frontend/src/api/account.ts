import { call, failure } from './client';

export type TeamRole = 'owner' | 'admin' | 'editor' | 'viewer';
export type ShareAccess = 'view' | 'edit';
export type CanvasAccess = 'view' | 'edit' | 'manage';

export type OwnerRequest = { kind: 'user' } | { kind: 'team'; team_id: string };

export interface Owner {
  kind: 'user' | 'team';
  id: string;
  team_name: string | null;
}

export interface User {
  id: string;
  email: string;
  name: string;
  created_at: string;
}

export interface Team {
  id: string;
  name: string;
  role: TeamRole;
  members: number;
  created_at: string;
}

export interface Me {
  user: User;
  teams: Team[];
}

export interface Member {
  user: User;
  role: TeamRole;
  joined_at: string;
}

export interface TeamDetail {
  team: Team;
  members: Member[];
}

export interface AuthConfig {
  enabled: boolean;
  registration_open: boolean;
}

export interface CanvasInfo {
  id: string;
  title: string;
  chain_id: number;
  version: number;
  owner: Owner;
  access: CanvasAccess;
  created_by: string;
  updated_by: string;
  created_at: string;
  updated_at: string;
}

export interface CanvasWithDocument {
  canvas: CanvasInfo;
  document: unknown;
}

export interface PublicCanvas {
  title: string;
  chain_id: number;
  updated_at: string;
  document: unknown;
}

export interface Share {
  user: User;
  access: ShareAccess;
  created_at: string;
}

export interface PublicLink {
  id: string;
  created_by: string;
  created_at: string;
  revoked_at: string | null;
}

export interface UserLabel {
  id: string;
  chain_id: number;
  address: string;
  value: string;
  note: string;
  owner: Owner;
  writable: boolean;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export const ROLE_TITLE: Record<TeamRole, string> = {
  owner: 'Owner',
  admin: 'Admin',
  editor: 'Editor',
  viewer: 'Viewer',
};

const JSON_HEADERS = { 'content-type': 'application/json' };

async function send<T>(path: string, method: string, body?: unknown): Promise<T> {
  const response = await call(path, {
    method,
    headers: body === undefined ? undefined : JSON_HEADERS,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) throw await failure(response);
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

const get = <T>(path: string) => send<T>(path, 'GET');
const post = <T>(path: string, body?: unknown) => send<T>(path, 'POST', body);
const patch = <T>(path: string, body: unknown) => send<T>(path, 'PATCH', body);
const put = <T>(path: string, body: unknown) => send<T>(path, 'PUT', body);
const remove = (path: string) => send<void>(path, 'DELETE');

export const accountApi = {
  config: () => get<AuthConfig>('/auth/config'),
  me: () => get<Me>('/auth/me'),
  register: (email: string, name: string, password: string) => post<Me>('/auth/register', { email, name, password }),
  login: (email: string, password: string) => post<Me>('/auth/login', { email, password }),
  logout: () => post<void>('/auth/logout'),
  logoutEverywhere: () => post<void>('/auth/logout-everywhere'),
  rename: (name: string) => patch<Me>('/auth/me', { name }),
  changePassword: (current: string, next: string) => post<Me>('/auth/password', { current, next }),
};

export const teamApi = {
  list: () => get<Team[]>('/teams'),
  create: (name: string) => post<Team>('/teams', { name }),
  detail: (id: string) => get<TeamDetail>(`/teams/${id}`),
  rename: (id: string, name: string) => patch<Team>(`/teams/${id}`, { name }),
  delete: (id: string) => remove(`/teams/${id}`),
  addMember: (id: string, email: string, role: TeamRole) => post<Member>(`/teams/${id}/members`, { email, role }),
  changeRole: (id: string, userId: string, role: TeamRole) => patch<void>(`/teams/${id}/members/${userId}`, { role }),
  removeMember: (id: string, userId: string) => remove(`/teams/${id}/members/${userId}`),
};

export const canvasApi = {
  list: () => get<CanvasInfo[]>('/canvases'),
  open: (id: string) => get<CanvasWithDocument>(`/canvases/${id}`),
  create: (title: string, chainId: number, owner: OwnerRequest, document: unknown) =>
    post<CanvasInfo>('/canvases', { title, chain_id: chainId, owner, document }),
  update: (id: string, version: number, change: { title?: string; chain_id?: number; document?: unknown }) =>
    patch<CanvasInfo>(`/canvases/${id}`, { version, ...change }),
  delete: (id: string) => remove(`/canvases/${id}`),
  shares: (id: string) => get<Share[]>(`/canvases/${id}/shares`),
  share: (id: string, email: string, access: ShareAccess) => put<Share>(`/canvases/${id}/shares`, { email, access }),
  unshare: (id: string, userId: string) => remove(`/canvases/${id}/shares/${userId}`),
  links: (id: string) => get<PublicLink[]>(`/canvases/${id}/links`),
  createLink: (id: string) => post<{ link: PublicLink; token: string }>(`/canvases/${id}/links`),
  revokeLink: (id: string, linkId: string) => remove(`/canvases/${id}/links/${linkId}`),
  openPublic: (token: string) => get<PublicCanvas>(`/public/canvases/${encodeURIComponent(token)}`),
};

export const labelApi = {
  list: (chainId?: number, addresses?: string[]) => {
    const params = new URLSearchParams();
    if (chainId !== undefined) params.set('chain_id', String(chainId));
    if (addresses && addresses.length > 0) params.set('addresses', addresses.join(','));
    const suffix = params.size > 0 ? `?${params}` : '';
    return get<UserLabel[]>(`/labels${suffix}`);
  },
  create: (chainId: number, address: string, value: string, note: string, owner: OwnerRequest) =>
    post<UserLabel>('/labels', { chain_id: chainId, address, value, note, owner }),
  update: (id: string, value: string, note: string) => patch<UserLabel>(`/labels/${id}`, { value, note }),
  delete: (id: string) => remove(`/labels/${id}`),
};

export function ownerName(owner: Owner): string {
  return owner.kind === 'team' ? (owner.team_name ?? 'Team') : 'Personal';
}

export function canManageTeam(role: TeamRole): boolean {
  return role === 'owner' || role === 'admin';
}

export function canEditTeam(role: TeamRole): boolean {
  return role !== 'viewer';
}

export function isUserLabelSource(source: string): boolean {
  return source === 'you' || source.startsWith('team:');
}
