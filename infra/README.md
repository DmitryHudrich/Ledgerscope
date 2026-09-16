# infra

Docker Compose stack for Ledgerscope: the Rust API and the frontend behind
nginx.

```bash
cp .env.example .env      # set ETH_RPC_URL
docker compose up --build
```

Then open <http://localhost:8080>. The API is on `127.0.0.1:3000` for direct
pokes (`curl 'http://127.0.0.1:3000/graph?wallet=0x…&from=21000000&to=21000010'`).

| Service | Image stage | Listens on |
|---|---|---|
| `api` | `debian:trixie-slim` + the `web-api` binary | `127.0.0.1:${API_PORT}`, default 3000 |
| `web` | `nginx-unprivileged` serving `frontend/dist` | `0.0.0.0:${WEB_PORT}`, default 8080 |
| `clickhouse` | `clickhouse/clickhouse-server` | `127.0.0.1:8123` (HTTP) and `:9000` (native) |

nginx proxies `/api/<path>` to the API's `/<path>`, the same rewrite the Vite dev
server does, so the frontend bundle needs no build-time API URL.

## Networking: why `network_mode: host`

Both services share the host network namespace, so `127.0.0.1` inside the
containers is your host's loopback.

That exists for the RPC proxy. `ETH_RPC_PROXY` typically points at a SOCKS proxy
running on the host and bound to `127.0.0.1` — and a bridged container cannot
reach that. On Linux `host.docker.internal` maps to the bridge gateway
(`172.17.0.1`), which a loopback-only listener never answers on. Host networking
is the only way to use such a proxy without reconfiguring it.

The trade: no network isolation, and host networking behaves differently on
macOS/Windows. The API is bound to loopback, so it stays off the LAN either way;
only `web` is externally reachable.

### Going back to an isolated bridge network

Worth doing if the proxy is not needed, or if it can listen on `0.0.0.0`
(or `172.17.0.1`) instead:

1. Drop `network_mode: host` from both services in `docker-compose.yml`.
2. Give `api` back its published port and host alias:
   ```yaml
   ports:
     - "127.0.0.1:${API_PORT:-3000}:3000"
   extra_hosts:
     - "host.docker.internal:host-gateway"
   ```
   and set `BIND_ADDR: 0.0.0.0:3000` so `web` can reach it.
3. Add `ports: ["${WEB_PORT:-8080}:8080"]` to `web`.
4. In `nginx.conf.template`, point `proxy_pass` at `http://api:3000/`.
5. In `.env`, use `ETH_RPC_PROXY=socks5h://host.docker.internal:2080`.

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
the RPC key). The API binary also takes a YAML file; see
[Config file](#config-file) below.

| Variable | Default | Notes |
|---|---|---|
| `ETH_RPC_URL` | — | Required; compose refuses to start without it. |
| `ETH_RPC_PROXY` | `socks5h://127.0.0.1:2080` | Host loopback, since the namespace is shared. Empty value = direct connection. |
| `ETH_RPC_RPS` | `15` | Client-side rate limit for the JSON-RPC calls. |
| `BIND_ADDR` | `127.0.0.1:${API_PORT}` | Set by compose; the binary's own default is the same. |
| `WEB_PORT` / `API_PORT` | 8080 / 3000 | Host ports. `WEB_PORT` must stay above 1024 — nginx runs unprivileged. |
| `RUST_LOG` | `info` | Filter for `tracing`, e.g. `info,adapters=debug`. |
| `CLICKHOUSE_DB` / `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` | `ledgerscope` | The image drops the `default` user once `CLICKHOUSE_USER` is set. |
| `LEDGERSCOPE_CONFIG` | `./config.yaml` | Path to the YAML config. Missing default path = env and built-in defaults only. |

### Config file

`config.example.yaml` in the repo root is the template:

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

The file is looked up at `LEDGERSCOPE_CONFIG`, or at `./config.yaml` when that
variable is unset — an explicit path must exist, the default one need not.
Unknown keys are rejected rather than ignored.

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

The compose stack passes configuration as environment variables, so it needs no
file. To use one there, mount it and point the variable at it:

```yaml
    volumes:
      - ../config.yaml:/etc/ledgerscope/config.yaml:ro
    environment:
      LEDGERSCOPE_CONFIG: /etc/ledgerscope/config.yaml
```

`WEB_PORT` and `API_PORT` are substituted into `nginx.conf.template` by the nginx
entrypoint at container start; `NGINX_ENVSUBST_FILTER` limits envsubst to those
two names so nginx's own `$uri`/`$host` survive.

## Caveats

- The API keeps one accumulating graph in process memory. Restarting the
  container drops it — nothing in the API reads ClickHouse yet; the service is
  up for the storage work, with its data in the `clickhouse-data` volume.
- `/graph` blocks while it seeds the range over JSON-RPC. nginx allows 900s for
  it; widen `proxy_read_timeout` in `nginx.conf.template` for huge ranges.
- Host networking means the ports are real host ports: `up` fails if something
  already holds `WEB_PORT` or `API_PORT`.
