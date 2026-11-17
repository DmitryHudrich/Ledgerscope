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

- [x] Address registry: contract → protocol, type, tokens, fee, factory (partial: Uniswap v2/v3/v4 and forks, verified on-chain via factory / PoolManager, persisted in ClickHouse)
- [ ] Normalized record: `{tx, logIndex, protocol, pool, trader, tokenIn, amountIn, tokenOut, amountOut}` (partial: swaps are graph interactions with pool, tokens and amounts; trader is `tx.from`, no `logIndex`)
- [x] Uniswap v2 + Sushi and forks (`Swap`, verify via `factory` / `getPair`)
- [x] Uniswap v3 (`Swap`, `factory.getPool`)
- [x] Uniswap v4 (PoolManager `0x000000000004444c5dc75cb358380d2e3de08a90`, `Swap`, pool by `id`, key read from the v4 PositionManager `poolKeys` and checked against the `id` hash)
- [ ] Uniswap v4 pools unknown to the PositionManager (created directly or only touched by other routers): `poolKeys` returns nothing, so tokens and fee stay unknown and the pool is rechecked once a minute. `Initialize` logs are not an option on public RPCs (`eth_getLogs` ranges from the PoolManager deploy block are rejected, 1000 to 100000 blocks max); needs an archive or keyed RPC, our own index of `Initialize` events, or tokens inferred from the transfers in the swap tx. The PositionManager address (`0xbD216513d74C8cf14cf4747E6AaA6420FF64ee9e`) is not verified against a live node yet: a wrong address only leaves v4 pools unresolved
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

### 7. Investigation: CEX theft, Nov 1 ($106M+, USDC/USDT)

Source: community alert. Funds were withdrawn from several CEX accounts as USDC/USDT, swapped USDC → ETH, sent to instant exchanges (WellWallet, Kyrrex, …), converted to XMR via Wagyu and bridged to other chains. The actor is unidentified; the alert's author says with high confidence it is not Lazarus.

Theft addresses (seeds):

```
0xADd95cD98E0288ba69785E3151E1db86adF8Dd58
0x29e9D7715D935e0D0D4B0DD0efC952A97E551020
0x378B4f150707C283574DC89739608D29036D57d5
0x795a9d22F233024487161Fe6daFf73397c162621
0x730Bcb1296758c377149d7409f98f041cf723992
0x12D9012731eb0D54871A31Af5566Eb9ECCa351Db
0x5A154f77852ed274Fd6F4c324a1Ee27809052FF7
0xA1096A66f9ed58A9594A6A1ad67A5472C0907224
0x90F0b6D51ed2FE2348A45b2C49BC441E959bc557
0x8A97BC406180e28b11C5495fA099BF5119c6C6B0
0xd010cba790a6ab0048da18f2fbe8e3faf463a16a
```

Constraint: `trace_*` / `debug_trace*` are not available, so native ETH has to be recovered from logs, calldata and balances.

#### Already in the TODO above, now blocking this case

- [ ] §1 WETH `Deposit` / `Withdrawal`: the main log-level signal that a swap paid out ETH
- [ ] §1 Transaction metadata (`tx.from`, `tx.to`, selector, `logIndex`): needed to attribute router payouts and to decode calldata
- [ ] §2 Aggregators (1inch, 0x, Paraswap) and CoW (`Trade`, trader = `owner`): USDC → ETH swaps of this size rarely go through a single pool
- [ ] §3 Bridges: Across, Stargate, CCTP, native L2 bridges
- [ ] §4 Labels for CEX hot wallets and bridges
- [ ] §6 Taint analysis (FIFO / haircut)
- [ ] §6 Collapse transit nodes (pool and router hidden, thief → thief flow)
- [ ] §6 Multichain: same address on L2s
- [ ] §6 Path finding between addresses and time windows

#### New for this case

- [ ] Native ETH without traces:
  - an EOA's outgoing ETH is always a top-level tx and already visible; only incoming internal ETH is lost
  - router payout: WETH `Withdrawal(src = router)` plus a swap in the same tx → ETH edge router → recipient
  - recipient from calldata: UniversalRouter `UNWRAP_WETH` / `SWEEP` / `PAY_PORTION`, V2/V3 router `unwrapWETH9(recipient)`, 1inch `unoswap*`; `MSG_SENDER` (`address(1)`) resolves to `tx.from`; fallback is `tx.from` with lower confidence
  - edge source `heuristic("eth_payout_from_logs", 1)` with the swap tx as evidence
  - verification: `eth_getBalance(addr, n-1)` vs `eth_getBalance(addr, n)` for blocks where a seed or traced address appears, compared with the visible in/out minus gas; a mismatch flags an unexplained internal transfer (needs an archive node)
- [ ] Case import: bulk upload of seed addresses with a shared source (`user` / `community_alert`), key `incident.theft`, note with a link to the alert, tied to a canvas
- [ ] Follow-the-money traversal: outgoing only, only after the funds arrived, through swaps (USDC in → ETH out to the same trader continues the trace), stop and mark at exits (CEX, instant exchange, XMR swap, bridge)
- [ ] Label set for instant exchanges and XMR swap services (WellWallet, Kyrrex, Wagyu, FixedFloat, ChangeNOW, SideShift, eXch, …) with `provider` provenance; keys `entity.instant_exchange`, `entity.xmr_swap`
- [ ] Cross-chain edges from bridge logs: Across `FundsDeposited` (`destinationChainId`, `recipient`), CCTP `DepositForBurn` (`destinationDomain`, `mintRecipient`); edge to the same or another address on the destination chain
- [ ] One request across several EVM chains for the same seed list
- [ ] Block window: `max_blocks` 100,000 ≈ 14 days on Ethereum, already less than the time since Nov 1; window per address from its first incoming transfer instead of one global range
- [ ] Watch seeds and traced addresses for new activity (the actor is still moving funds)
- [ ] USDT `AddedBlackList` / USDC `Blacklisted` events: show which balances are already frozen
- [ ] Case report export: addresses, evidence txs, amounts in `U256`, tag provenance

### Notes on the analyzed graph

- Main node `0xd661623947ef2ead12d577b47021c167d163b0c5`: EOA, ~19,000 transfers in 33 hours, ~$72M turnover in WETH/USDC. Looks like a market maker or arbitrageur; no labels in the export.
- `0x2192bc3b4028acc1113f2cd9ac2cba70c36520db`: intermediary contract, USDC and WETH pass through it at near-zero balance; 413 failed calls from `0x7405…` and `0x0927…`, reason unclear.
