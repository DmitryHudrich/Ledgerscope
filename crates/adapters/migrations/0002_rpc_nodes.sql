create table rpc_nodes (
    id uuid primary key,
    user_id uuid not null references users (id) on delete cascade,
    chain_id bigint not null,
    url text not null,
    created_at timestamptz not null
);

create unique index rpc_nodes_unique_url on rpc_nodes (user_id, chain_id, url);
create index rpc_nodes_user_idx on rpc_nodes (user_id, chain_id);
