# Ledgerscope

EVM and Tron transaction analysis: address graph, labels and actor classification, plus a passive TLS observer.

## TODO: tracer (swaps, DEX, labels)

Context: analysis of a LedgerScope export (Ethereum, blocks 26306142–26323955). The export only contains ERC-20 transfers and failed calls, so swaps and protocols have to be reconstructed by us. The export is capped at 20,000 transactions (~55% of the range covered).

### 0. What LedgerScope already has

#### Loading and storage

- [x] RPC node pool: rotation, per-node limits, retries, call monitoring, user's own nodes
- [x] Blocks via batched `eth_getBlockByNumber` + `eth_getBlockReceipts` (10 blocks per batch)
- [x] Parallel windowed loading, batched writes to ClickHouse, indexed-block cache in Redis
- [x] AWS public blockchain dataset (S3, parquet): on-disk day cache, enabled via config and the `aws-public` cargo feature
- [x] Per-block source in the index (rpc / dataset), re-fetch via RPC for exact wei
- [x] Index coverage and histogram, confirmation before loading missing blocks
- [x] Progress: RPC and dataset scan (with file download in MB), graph build stages, stop loading
- [x] Multichain: EVM and L2s from chainlist, TRON
- [ ] AWS: download both daily files in parallel and ahead of time, parse each day once, parse parquet on separate threads

#### Graph

- [x] Graph from addresses with traversal depth
- [x] "All transactions in block range" mode
- [x] Multiple block ranges: each with its own source, stats (exact / approximate / not indexed) and a latest button
- [x] Autofill of already indexed ranges
- [x] Filters: transaction types, reverted, self-transfers, minimum amount, single asset, wallets / contracts, labels, minimum counterparties, minimum transactions per flow, maximum hops from root, tags
- [x] Timeline: block display window via slider and bound inputs
- [x] Wallet search on canvas: address, partial address, TRON base58, labels, tags
- [x] Selection with hotkeys: node with neighbors, grow and shrink, neighbors, connected component, invert
- [x] Groups, hiding nodes and transactions, focus and dimming, annotations
- [x] Transaction table, node and edge inspectors, PNG and JSON export
- [x] Fast rendering of large graphs

#### Labels and heuristics

- [x] Address labels: etherscan-json, user, team
- [x] Tag core: every tag has a source (heuristic / provider / user / agent), version, confidence, evidence transactions
- [x] Heuristics: TRON energy rental, gas top-up and gas sponsor, fan-in / fan-out, exchange deposit addresses, categories from label keywords (exchange, bridge, mixer, DEX, lending, sanctions)
- [x] IP ↔ wallet from TLS observations
- [ ] Store tags in Postgres (currently computed on the fly per request)
- [ ] External address identification API (choose: Etherscan, Arkham, open label sets)
- [ ] Claude via MCP: `get_graph`, `get_tags`, `explain_address`, `propose_tag`, tags with the "agent" source
- [ ] Derived flow tags: "exchange inflow", "via bridge"

#### Accounts and canvases

- [x] Accounts, teams, roles
- [x] Saved canvases, sharing, public links
- [x] Snapshots and autosaves (last 20), open by canvas setting or a specific version
- [x] Start screen: last canvas, local session or account sign-in

### 1. Data (do this first)

- [ ] Native ETH via traces (`trace_block` / `debug_traceTransaction`)
- [ ] Transaction metadata: `tx.from`, `tx.to`, selector from input, gas, nonce, `logIndex`
- [ ] WETH `Deposit` / `Withdrawal` (wrap/unwrap)
- [ ] USD prices at block time
- [x] Source: own Erigon/Reth, RPC provider or BigQuery `crypto_ethereum.logs` / `traces` (currently: pool of public and own RPCs + AWS public dataset)

### 2. Swap decoders

- [ ] Address registry: contract → protocol, type, tokens, fee, factory
- [ ] Normalized record: `{tx, logIndex, protocol, pool, trader, tokenIn, amountIn, tokenOut, amountOut}`
- [ ] Uniswap v2 + Sushi and forks (`Swap`, verify via `factory` / `getPair`)
- [ ] Uniswap v3 (`Swap`, `factory.getPool`)
- [ ] Uniswap v4 (PoolManager `0x000000000004444c5dc75cb358380d2e3de08a90`, `Swap`, pool by `id` from `Initialize`)
- [ ] Curve (`TokenExchange`, `TokenExchangeUnderlying`, MetaRegistry for coin indices)
- [ ] Balancer v2 (Vault, `Swap`, `poolId`)
- [ ] CoW Protocol (GPv2Settlement, `Trade`, trader = `owner`, not the solver)
- [ ] Aggregators (1inch etc.): router list by `tx.to`, route as a chain of swaps within one tx, no double-counting of hops
- [ ] RFQ (Bebop, Hashflow, 0x RFQ, UniswapX): settlement contract events + "opposing transfers without a pool" heuristic
- [ ] Unknown pools: try `token0()`, `token1()`, `factory()`
- [ ] Cross-check against Dune `dex.trades`

### 3. Other protocols

- [ ] Bridges (Across, Stargate, LayerZero, Wormhole, native L2 bridges)
- [ ] ERC-4337 (bundler ≠ user) and Safe multisigs
- [ ] EIP-7702: delegated EOAs break the simple EOA / contract split
- [ ] Lending / staking: Aave, Compound, Morpho, Lido (rebasing tokens, stETH)
- [ ] Permit2, NFTs (ERC-721/1155), flash loans
- [ ] Mixers (Tornado Cash etc.)

### 4. Entities and labels

- [ ] Address labels: CEX wallets, pools, routers, bridges, bots, sanctions lists (partial: categories from provider label keywords)
- [ ] Clustering: common gas source, deposit address reuse, synchronized activity (partial: `gas_top_up` and `deposit_sweep` heuristics)
- [ ] Node classification: pool / router / user / bot / transit contract

### 5. Reliability

- [ ] Spam token and address poisoning filter
- [ ] Pagination and an explicit "export incomplete" flag (20,000 tx limit)
- [ ] Reorgs, dedup by `(tx, logIndex)`, incremental indexing and cache (partial: incremental indexing, cache and transaction dedup in ClickHouse exist, reorgs do not)

### 6. Analysis and graph

- [ ] Taint analysis (FIFO / haircut)
- [ ] Collapse transit nodes (hide pools and routers, show trader → trader flow)
- [ ] Path finding between addresses and time windows
- [ ] Multichain: same address on L2s

### Notes on the analyzed graph

- Main node `0xd661623947ef2ead12d577b47021c167d163b0c5`: EOA, ~19,000 transfers in 33 hours, ~$72M turnover in WETH/USDC. Looks like a market maker or arbitrageur; no labels in the export.
- `0x2192bc3b4028acc1113f2cd9ac2cba70c36520db`: intermediary contract, USDC and WETH pass through it at near-zero balance; 413 failed calls from `0x7405…` and `0x0927…`, reason unclear.
