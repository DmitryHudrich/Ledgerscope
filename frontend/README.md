# Ledgerscope frontend

Interactive transaction-graph UI for the `ledgerscope-web-api` crate. Addresses are
nodes, bundled transfers between a pair of addresses are edges; the layout is a
force simulation rendered to a single canvas.

## Run

```bash
npm install
npm run dev          # http://localhost:5173
```

The dev server proxies `/api/*` to `http://127.0.0.1:3000` (the address
`crates/web-api` binds to), so no CORS layer is needed on the Rust side. Point it
somewhere else with `LEDGERSCOPE_API=http://host:port npm run dev`.

Start the backend in another shell:

```bash
ETH_RPC_URL=<your-rpc-url> cargo run -p ledgerscope-web-api
```

**Demo data** in the top bar renders a generated sample instead of calling the
backend — useful when you have no RPC endpoint handy. It is on by default.

Other scripts: `npm run build` (typecheck + production bundle into `dist/`),
`npm run preview`, `npm run typecheck`.

## What it draws

`GET /graph?wallet=&from=&to=` returns `{ nodes: string[], edges: Tx[] }`. The UI
folds that into:

- **Nodes** — one per address. Area encodes ETH turnover (with a floor derived
  from connectivity, so a busy zero-value hub stays visible). Kind is carried by
  both colour *and* shape: the queried wallet is a ringed circle, a plain wallet
  a circle, a contract a rounded square. Contract-ness is inferred from calldata
  landing on the address, since the API exposes no code flag.
- **Edges** — one per ordered `(from → to)` pair, carrying every transaction
  between them. Stroke width encodes value moved; a dashed stroke means the
  bundle is calldata only. Reciprocal pairs bow to opposite sides. Direction is
  redundant: arrowhead plus animated dots.

Selecting or hovering a node dims everything outside its neighbourhood. The
transaction sheet at the bottom is the table view of the same data and can be
scoped to the selected address.

### Controls

| | |
|---|---|
| wheel / drag | zoom / pan |
| drag a node | pins it where you drop it |
| double-click a node | unpins it |
| click a node | select; `Esc` clears |
| `/` | focus the highlight box |
| `f` | fit to view |
| `space` | freeze / resume the layout |

## Notes

- `/graph` seeds every block in the range over JSON-RPC before it answers, so a
  wide range can take minutes. The request has no client-side timeout.
- The backend keeps one accumulating graph in process memory: a second query
  returns the union of everything seeded so far, not just the new range.
- Colours come from the validated categorical palette (slots 1–3), stepped
  separately for the light and dark surface rather than flipped between them.
