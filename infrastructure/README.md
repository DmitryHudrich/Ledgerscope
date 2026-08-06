# Running the stack with Docker Compose

Everything — SPA, C# public API, Rust engine, Postgres, Redis, observability,
and the egress proxy — comes up from this directory.

```sh
cd infrastructure
cp .env.example .env      # fill in ETHERSCAN_API_KEY + ALCHEMY_API_KEY
docker compose up -d --build
```

| Service    | URL                       | Notes                                        |
|------------|---------------------------|----------------------------------------------|
| `frontend` | http://localhost:8081     | nginx: SPA + same-origin `/api` and `/hubs`  |
| `accounts` | http://localhost:5107     | C# public API, Scalar UI at `/scalar`        |
| `engine`   | http://localhost:8080     | Rust engine, Scalar UI at `/scalar`          |
| `grafana`  | http://localhost:3000     | admin/admin                                  |

Host ports are overridable (`FRONTEND_PORT`, `ACCOUNTS_PORT`, `ENGINE_PORT`).

## Request path

```
browser -> frontend (nginx)
             /api/*  -> accounts:8080  (prefix stripped, as in the vite dev proxy)
             /hubs/* -> accounts:8080  (websocket upgrade for the SignalR graph hub)
                          accounts -> engine:8080        (X-Api-Key = ENGINE_API_KEY)
                          accounts -> postgres, redis
                                       engine -> postgres
                                       engine -> proxy:1080 -> external chain APIs
```

## Proxy for external APIs

The engine never dials the internet directly. Every outbound call — etherscan,
alchemy, moralis, trongrid, eth-labels.com — goes through the `proxy` service
(gost, SOCKS5 on `:1080`), wired in as
`PROXY__SOCKS_URL=socks5h://proxy:1080`. `socks5h` means the proxy also
resolves DNS, so upstream hostnames are never leaked to the local resolver.

By default the proxy egresses directly. To chain it to your own upstream, set
one gost forward flag in `.env`:

```sh
PROXY_UPSTREAM_ARG=-F socks5://host.docker.internal:2080   # proxy running on the host
PROXY_UPSTREAM_ARG=-F socks5://user:pass@vpn.example:1080  # remote SOCKS5
PROXY_UPSTREAM_ARG=-F http://squid.internal:3128           # HTTP CONNECT proxy
```

`host.docker.internal` is mapped to the host gateway on the proxy container, so
a proxy listening on the host (the `socks5h://127.0.0.1:2080` from
`config.yaml`) is reachable as `host.docker.internal:2080`.

Verify the path end to end:

```sh
docker compose exec proxy sh -c 'nc -z 127.0.0.1 1080 && echo socks5 up'
docker compose logs engine | grep "SOCKS proxy"      # -> "SOCKS proxy enabled"
```

To *enforce* proxy-only egress, add `internal: true` to the `backend` network
and attach the `proxy` service to a second, non-internal network. Then nothing
but the proxy can reach the internet at all. Published host ports do not
survive that change, so keep it for deployments that front the stack with a
separate ingress.

### Build-time proxy

`BUILD_HTTP_PROXY` / `BUILD_HTTPS_PROXY` / `BUILD_NO_PROXY` are passed as build
args to all three Dockerfiles and cover `apt`, `cargo`, `dotnet restore` and
`npm ci`. They apply to builder stages only and are absent from the runtime
images. Unrelated to the runtime SOCKS proxy above.

## Configuration

The engine reads `code/aml-core/config.yaml`, bind-mounted read-only at
`/app/config.yaml`, so chain wiring, heuristics, and scoring can be edited with
a `docker compose restart engine` instead of a rebuild. Host-specific values
are overridden by environment (`config` maps `A__B` to `a.b`):

| Env                        | Overrides                | Value in compose             |
|----------------------------|--------------------------|------------------------------|
| `DATABASE__URL`            | `database.url`           | `postgres` service           |
| `TELEMETRY__OTLP_ENDPOINT` | `telemetry.otlp_endpoint`| `http://tempo:4317`          |
| `PROXY__SOCKS_URL`         | `proxy.socks_url`        | `socks5h://proxy:1080`       |
| `SERVER__API_KEY`          | `server.api_key`         | `ENGINE_API_KEY`             |
| `*__API_KEY`               | provider keys            | from `.env`                  |
| `*__FILE_CACHE__DIR`       | provider disk caches     | `/data` (`engine_cache` vol) |

`ETHERSCAN_API_KEY` and `ALCHEMY_API_KEY` are **required**: the shipped
`chains:` config routes Ethereum through both, and the engine exits at startup
with `routed references '<provider>' but <provider> has no api keys` if either
is missing.

Both services own their schema migrations and run them at startup — refinery
for the engine (public schema), EF Core for accounts (`accounts` schema) — so
a fresh volume needs no manual step.

## Images

All three application images are multi-stage and drop to a non-root user.

- **engine** — `cargo-chef` splits dependency compilation from application
  compilation, so a source edit rebuilds one crate instead of the whole
  dependency graph (alloy, aws-lc-rs, jemalloc). Builder needs
  `build-essential` (jemalloc), `cmake` + `perl` (AWS-LC); the runtime is
  `debian:bookworm-slim` with just `ca-certificates` and `curl`.
- **accounts** — csproj-only restore layer, then `dotnet publish` onto
  `aspnet:10.0`.
- **frontend** — `npm ci` layer, `vite build`, then the static bundle onto
  `nginx:alpine` with `nginx.conf` replacing the vite dev proxy.

## Common operations

```sh
docker compose up -d --build          # start everything
docker compose logs -f engine         # follow one service
docker compose build engine           # rebuild after a Rust change (deps cached)
docker compose restart engine         # pick up a config.yaml edit
docker compose down                   # stop, keep volumes
docker compose down -v                # stop and wipe Postgres/Redis/caches
```
