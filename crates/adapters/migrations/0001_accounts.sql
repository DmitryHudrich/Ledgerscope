create table users (
    id uuid primary key,
    email text not null,
    name text not null,
    password_hash text not null,
    created_at timestamptz not null,
    updated_at timestamptz not null default now()
);

create unique index users_email_key on users (email);

create table sessions (
    digest bytea primary key,
    user_id uuid not null references users (id) on delete cascade,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);

create index sessions_user_idx on sessions (user_id);
create index sessions_expiry_idx on sessions (expires_at);

create table teams (
    id uuid primary key,
    name text not null,
    created_at timestamptz not null
);

create table team_members (
    team_id uuid not null references teams (id) on delete cascade,
    user_id uuid not null references users (id) on delete cascade,
    role text not null check (role in ('owner', 'admin', 'editor', 'viewer')),
    joined_at timestamptz not null default now(),
    primary key (team_id, user_id)
);

create index team_members_user_idx on team_members (user_id);

create table canvases (
    id uuid primary key,
    owner_user_id uuid references users (id) on delete cascade,
    owner_team_id uuid references teams (id) on delete cascade,
    title text not null,
    chain_id bigint not null,
    document jsonb not null,
    version bigint not null,
    created_by uuid not null references users (id),
    updated_by uuid not null references users (id),
    created_at timestamptz not null,
    updated_at timestamptz not null,
    check ((owner_user_id is null) <> (owner_team_id is null))
);

create index canvases_owner_user_idx on canvases (owner_user_id) where owner_user_id is not null;
create index canvases_owner_team_idx on canvases (owner_team_id) where owner_team_id is not null;

create table canvas_shares (
    canvas_id uuid not null references canvases (id) on delete cascade,
    user_id uuid not null references users (id) on delete cascade,
    access text not null check (access in ('view', 'edit')),
    created_at timestamptz not null default now(),
    primary key (canvas_id, user_id)
);

create index canvas_shares_user_idx on canvas_shares (user_id);

create table canvas_links (
    id uuid primary key,
    canvas_id uuid not null references canvases (id) on delete cascade,
    digest bytea not null unique,
    created_by uuid not null references users (id),
    created_at timestamptz not null,
    revoked_at timestamptz
);

create index canvas_links_canvas_idx on canvas_links (canvas_id);

create table labels (
    id uuid primary key,
    owner_user_id uuid references users (id) on delete cascade,
    owner_team_id uuid references teams (id) on delete cascade,
    chain_id bigint not null,
    address bytea not null check (length(address) = 20),
    value text not null,
    note text not null default '',
    created_by uuid not null references users (id),
    created_at timestamptz not null,
    updated_at timestamptz not null,
    check ((owner_user_id is null) <> (owner_team_id is null))
);

create unique index labels_unique_value on labels (
    coalesce(owner_user_id, owner_team_id), chain_id, address, lower(value)
);
create index labels_address_idx on labels (chain_id, address);
create index labels_owner_user_idx on labels (owner_user_id) where owner_user_id is not null;
create index labels_owner_team_idx on labels (owner_team_id) where owner_team_id is not null;
