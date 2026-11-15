# nile-send

Sends test TRX transfers to the **Tron Nile testnet** through the observer's
SOCKS proxy, so the observer sniffs a real broadcast session and posts an
observation. Pairs with the `TLS deanon` dialog in the UI.

## Prerequisites

1. **A funded Nile account.** Grab test TRX at https://nileex.io/join/getJoinPage
   and keep its **hex private key**.
2. **The observer running in proxy mode** with the Nile group, e.g. `observer.yaml`:
   ```yaml
   mode: proxy
   listen: 0.0.0.0:8443
   api: http://127.0.0.1:3000
   token: devnet-observer-token
   default_port: 443
   groups:
     - family: tron
       chain_id: 3448148188
       hosts: [api.nile.trongrid.io, nile.trongrid.io, api.nileex.io]
   ```
   ```bash
   cargo run -p ledgerscope-observer -- --config observer.yaml
   ```
3. **web-api with ingest on**: `OBSERVER_ENABLED=true` and a matching
   `OBSERVER_TOKEN`, Nile served (`CHAINS_TRON=true`, `CHAINS_TESTNETS=true`).

## Run

```bash
cd scripts/nile-send
npm install
```

The script uses plain tronweb; traffic is routed through the observer by the
proxy, not by touching tronweb internals. Two ways, both hitting the observer on
`:8443` (it auto-detects HTTP CONNECT and SOCKS on the same port):

**A. `HTTPS_PROXY` env (recommended, no extra tools).** tronweb's axios tunnels
through the observer's HTTP CONNECT:
```bash
HTTPS_PROXY=http://127.0.0.1:8443 PRIVATE_KEY=<your-nile-hex-key> COUNT=5 node send.mjs
```

**B. proxychains-ng (bulletproof, routes the whole process via SOCKS).** Works
regardless of what the HTTP client does:
```bash
PRIVATE_KEY=<your-nile-hex-key> COUNT=5 proxychains4 -f proxychains.conf node send.mjs
```

The sender address it prints is the wallet that gets linked to your IP in the UI.
With no `TO` it self-transfers (owner address = sender), which is enough for the
heuristic.

## Options (env vars)

| var | default | meaning |
|---|---|---|
| `PRIVATE_KEY` | — | funded Nile account, hex (required) |
| `TO` | sender | recipient base58 address |
| `AMOUNT_SUN` | `1000000` | amount in sun (1 TRX = 1_000_000) |
| `COUNT` | `3` | how many transfers to send |
| `DELAY_MS` | `4000` | pause between transfers |
| `POLL` | `true` | poll `gettransactioninfobyid` after each (more traffic) |
| `HTTPS_PROXY` | — | observer HTTP-CONNECT endpoint, e.g. `http://127.0.0.1:8443` |
| `NILE_FULLHOST` | `https://api.nile.trongrid.io` | real Nile node (TLS terminates here) |

## What to watch

- **Observer log:** `… -> api.nile.trongrid.io [chain 3448148188]: broadcast|query (0.xx)`.
- **UI → TLS deanon → Nile:** set `from block` = current head − 300 (get it with
  `curl -s https://nile.trongrid.io/wallet/getnowblock | jq .block_header.raw_data.number`),
  then **Correlate**. The sender address shows up linked to the observer-seen IP,
  and a colored dot appears over that node on the graph.

If the observer log stays empty: make sure you ran with `HTTPS_PROXY=...` (or under
proxychains) — without it the script warns and talks to Nile directly, bypassing
the observer. As a last resort, point a browser/system proxy at the observer and
send from TronLink on Nile.
