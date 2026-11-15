create table tls_observations (
    id uuid primary key,
    src_ip text not null,
    chain_id bigint not null,
    intent text not null,
    confidence double precision not null,
    observed_at timestamptz not null
);

create index tls_observations_match_idx on tls_observations (chain_id, intent, observed_at);

create table ip_wallet_links (
    id uuid primary key,
    src_ip text not null,
    chain_id bigint not null,
    address bytea not null check (length(address) = 20),
    hits integer not null,
    confidence double precision not null,
    first_seen timestamptz not null,
    last_seen timestamptz not null
);

create unique index ip_wallet_links_unique on ip_wallet_links (src_ip, chain_id, address);
create index ip_wallet_links_chain_idx on ip_wallet_links (chain_id);

create table tls_correlation_state (
    chain_id bigint primary key,
    watermark_block bigint not null,
    updated_at timestamptz not null default now()
);
