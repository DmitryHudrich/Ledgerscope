# infra

Docker Compose stack for Ledgerscope: the Rust API and the frontend behind
Caddy, which also terminates TLS.

```bash
cp .env.example .env      # set DOMAIN and the secrets
docker compose up --build
```

Two env files, both gitignored: `.env` for local development (read by default)
and `.env.prod` for production. Pick the production one explicitly, or copy it
to `.env` on the server:

```bash
docker compose --env-file .env.prod up -d --build
```

Then open `https://$DOMAIN` (<https://localhost> by default). The API is on `127.0.0.1:3000` for direct
pokes (`curl 'http://127.0.0.1:3000/graph?wallet=0x…&from=21000000&to=21000010'`).

| Service | Image stage | Published on the host |
|---|---|---|
| `api` | `debian:trixie-slim` + the `web-api` binary | `127.0.0.1:${API_PORT}`, default 3000 |
| `web` | `caddy` serving `frontend/dist` | `80`, `443`, `443/udp` on all interfaces |
| `anvil` | `foundry`, profile `devnet` | `127.0.0.1:${DEVNET_PORT}`, default 8545 |
| `tron-devnet` | `trontools/quickstart`, profile `tron-devnet` | `127.0.0.1:${DEVNET_TRON_PORT}`, default 9090 |
| `clickhouse`, `postgres`, `redis` | upstream images | not published |

Caddy proxies `/api/<path>` to the API's `/<path>`, the same rewrite the Vite dev
server does, so the frontend bundle needs no build-time API URL. Everything else
is the SPA: `/assets/*` is cached for a year, other paths fall back to
`index.html` with `Cache-Control: no-cache`.

## Devnets

The local devnets are off by default and start only with their profile:

```bash
docker compose --profile devnet up -d                          # anvil, EVM chain 31337
docker compose --profile tron-devnet up -d                     # local tron node
docker compose --profile devnet --profile tron-devnet up -d    # both
```

The API also needs `CHAINS_DEVNETS=true`. It probes the devnets once at startup
and leaves out any that does not answer, so a devnet started after the API is
only picked up after `docker compose restart api`. Started together, the API
waits for anvil to become healthy first.

## TLS

The site address is `DOMAIN` from `infra/.env`. For a public hostname Caddy gets
and renews a Let's Encrypt certificate on its own: the hostname's DNS has to
point at this host, and ports 80 and 443 have to be reachable from the
internet. Certificates live in the `caddy-data` volume; keep it, or every
restart asks Let's Encrypt again and runs into its rate limits.

`DOMAIN=localhost` gets a certificate from Caddy's local CA instead, which the
browser will not trust until that CA is installed.

## Networking

The stack runs on the compose bridge network. Services talk to each other by
name (`clickhouse:8123`, `postgres:5432`, `redis:6379`, `anvil:8545`,
`tron-devnet:9090`, `api:3000`), and compose builds those URLs itself, so
`infra/.env` holds no service addresses. The databases have no host ports at
all; only `web` is reachable from outside, everything else that is published
binds host loopback.

Caddy resolves `api` through Docker's DNS on every new connection, so
recreating the API container does not leave it pointing at a stale address.

An RPC proxy running on the host is reachable as `host.docker.internal`, e.g.
`ETH_RPC_PROXY=socks5h://host.docker.internal:2080`. The proxy has to listen on
`0.0.0.0` or the bridge gateway (`172.17.0.1`): a loopback-only listener never
answers a container.

## Build caching

`api.Dockerfile` uses [cargo-chef](https://github.com/LukeMathWalker/cargo-chef).
The `planner` stage distills the workspace into a `recipe.json` — a synthetic
manifest with no source code in it — and the `builder` stage runs
`cargo chef cook` against that recipe alone. Because the recipe's content only
changes when a dependency changes, editing crate sources leaves the dependency
layer cached and rebuilds just the workspace crates.

Layer order, cheapest invalidation last:

1. `cargo install cargo-chef` — changes only with `RUST_VERSION`.
2. `cargo chef cook --release` — changes only with `Cargo.toml` / `Cargo.lock`.
3. `cargo build --release --bin web-api` — changes with any source edit.

The frontend image does the equivalent with `npm ci` keyed on
`package-lock.json` before the sources are copied.

Both builds run from the repo root as context; `.dockerignore` keeps `target/`
(1.8 GB locally) and `node_modules/` out of it.

## Configuration

For the compose stack everything comes from `infra/.env` (gitignored — it holds
the RPC key). The containers never read a YAML file: the image ships the binary
alone, `.dockerignore` keeps `config.yaml` out of the build context, and the
entrypoint passes no `--config`. Outside Docker the binary also takes a YAML
file; see [Config file](#config-file) below.

| Variable | Default | Notes |
|---|---|---|
| `ETH_RPC_URL` | — | Optional with `RPC_CHAINLIST=true`. |
| `ETH_RPC_PROXY` | empty | Direct connection. A host proxy is `host.docker.internal`. |
| `ETH_RPC_RPS` | `15` | Client-side rate limit for the JSON-RPC calls. |
| `BIND_ADDR` | `0.0.0.0:3000` | Fixed by compose inside the container; the binary's own default is `127.0.0.1:3000`. |
| `DOMAIN` | `localhost` | Caddy's site address; see [TLS](#tls). |
| `API_PORT` | 3000 | Host loopback port of the API, for the observer agent and direct pokes. Inside the container the API is always on 3000. |
| `RUST_LOG` | `info` | Filter for `tracing`, e.g. `info,adapters=debug`. |
| `CLICKHOUSE_DB` / `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` | `ledgerscope` | The image drops the `default` user once `CLICKHOUSE_USER` is set. |

### Config file

This applies to running the binary directly — the compose stack is environment
only. `config.example.yaml` in the repo root is the template:

```yaml
server:
  bind_addr: ${BIND_ADDR:-127.0.0.1:3000}

log:
  filter: ${RUST_LOG:-info}

eth:
  rpc:
    url: ${ETH_RPC_URL:?set it in infra/.env or export it}
    proxy: ${ETH_RPC_PROXY-socks5h://127.0.0.1:2080}
    rps: ${ETH_RPC_RPS:-15}
```

Values are read in this order, last one wins: built-in defaults, the YAML file,
then the environment variables from the table above. So a key left out of the
YAML is not an error, and `ETH_RPC_URL=… cargo run` keeps working with no file
at all.

The path comes from `--config` (`-c`):

```bash
web-api --config /etc/ledgerscope/config.yaml
```

Without the flag the binary falls back to `./config.yaml` — an explicit path
must exist, the default one need not. Unknown keys are rejected rather than
ignored.

Placeholders are expanded inside the YAML before it is parsed:

| Form | Meaning |
|---|---|
| `${NAME}` | Fails if `NAME` is unset. |
| `${NAME-default}` | `default` when `NAME` is unset. |
| `${NAME:-default}` | `default` when `NAME` is unset **or** empty. |
| `${NAME:?message}` | Fails with `message` when `NAME` is unset or empty. |
| `$${NAME}` | Literal `${NAME}`. |

Defaults nest (`${A:-${B:-last}}`). A scalar that is exactly one placeholder
keeps its type, so `rps: ${ETH_RPC_RPS:-15}` stays a number; anything with text
around the placeholder is a string.

The compose stack passes configuration as environment variables only; the API
container is deliberately file-free.

## Caveats

- The API keeps one accumulating graph in process memory. Restarting the
  container drops it — nothing in the API reads ClickHouse yet; the service is
  up for the storage work, with its data in the `clickhouse-data` volume.
- `/graph` blocks while it seeds the range over JSON-RPC. Caddy sets no
  response timeout on the proxy, so a wide range is only bounded by the client.
- `up` fails if something on the host already holds 80, 443, `API_PORT` or
  `DEVNET_PORT`.
