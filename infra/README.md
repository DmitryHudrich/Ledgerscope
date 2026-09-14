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

Everything comes from `infra/.env` (gitignored — it holds the RPC key).

| Variable | Default | Notes |
|---|---|---|
| `ETH_RPC_URL` | — | Required; compose refuses to start without it. |
| `ETH_RPC_PROXY` | `socks5h://127.0.0.1:2080` | Host loopback, since the namespace is shared. Empty value = direct connection. |
| `BIND_ADDR` | `127.0.0.1:${API_PORT}` | Set by compose; the binary's own default is the same. |
| `WEB_PORT` / `API_PORT` | 8080 / 3000 | Host ports. `WEB_PORT` must stay above 1024 — nginx runs unprivileged. |
| `RUST_LOG` | `info` | |

`WEB_PORT` and `API_PORT` are substituted into `nginx.conf.template` by the nginx
entrypoint at container start; `NGINX_ENVSUBST_FILTER` limits envsubst to those
two names so nginx's own `$uri`/`$host` survive.

## Caveats

- The API keeps one accumulating graph in process memory. Restarting the
  container drops it; there is no persistence, so no volumes are declared.
- `/graph` blocks while it seeds the range over JSON-RPC. nginx allows 900s for
  it; widen `proxy_read_timeout` in `nginx.conf.template` for huge ranges.
- Host networking means the ports are real host ports: `up` fails if something
  already holds `WEB_PORT` or `API_PORT`.
