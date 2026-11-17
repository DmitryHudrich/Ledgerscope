alter table canvases add column opening text not null default 'latest'
    check (opening in ('latest', 'snapshot'));

create table canvas_versions (
    id uuid primary key,
    canvas_id uuid not null references canvases (id) on delete cascade,
    kind text not null check (kind in ('snapshot', 'autosave')),
    label text not null default '',
    chain_id bigint not null,
    canvas_version bigint not null,
    document jsonb not null,
    created_by uuid not null references users (id) on delete cascade,
    created_at timestamptz not null
);

create index canvas_versions_canvas_idx on canvas_versions (canvas_id, kind, created_at desc);
