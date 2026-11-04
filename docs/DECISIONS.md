# Decisions

## DBC SDK pin (3 Nov 2026)

**Pinned:** `@meteora-ag/dynamic-bonding-curve-sdk@2.0.0`  
**Related:** `@meteora-ag/cp-amm-sdk@1.5.1`  
**Program:** DBC `0.2.2` at `dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN` (same id on mainnet and **devnet**)

Jume's rule: use the **latest** SDK that matches the **devnet** program. Only fall back to 2.0.0, then 1.5.11, if the newest fails that check. On 3 Nov 2026 the newest *is* 2.0.0, and it matches. No fallback.

### npm (checked 3 Nov 2026)

| Package | `dist-tags.latest` | Published | Other tags we did not use |
| --- | --- | --- | --- |
| `@meteora-ag/dynamic-bonding-curve-sdk` | **2.0.0** | 15 Oct 2026 | `1.5.13` (24 Sep 2026), `1.5.11` (30 Jul 2026), `next`/`beta` RCs |
| `@meteora-ag/cp-amm-sdk` | **1.5.1** | 30 Sep 2026 | `beta`/`next` RCs behind 1.5.1 |

Command: `npm view @meteora-ag/dynamic-bonding-curve-sdk dist-tags version time` (and the same for `cp-amm-sdk`). There is no 2.0.1+ and no newer `latest` than 2.0.0.

### What we verified on Solana **devnet** (read-only)

Script: `pnpm verify-dbc-sdk` (`relayer/src/verify-dbc-sdk.ts`). RPC: `https://api.devnet.solana.com`.

| Check | Result |
| --- | --- |
| DBC program `getAccountInfo` | executable, owner `BPFLoaderUpgradeab1e11111111111111111111111`, 211 141 440 lamports, 36-byte program account |
| DAMM v2 program | executable, same loader; `CP_AMM_PROGRAM_ID` from cp-amm-sdk 1.5.1 equals `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG`; `new CpAmm(connection)` constructs |
| SDK-embedded IDL | `metadata.version` **0.2.2**, `address` = DBC program id, 35 instructions, 10 accounts, includes `createConfig2` / `create_config2` |
| Instruction discriminators | `initialize_virtual_pool_with_spl_token` `[140,85,215,176,102,54,104,79]`, `swap` `[248,198,158,145,225,117,135,200]`, `create_config2` `[9,202,24,203,143,212,252,81]` — each equals `sha256("global:<name>")[0..8]` and the PLAN.md 0.2.2 values |
| Account discriminators | `PoolConfig` `[26,108,14,123,116,230,129,43]`, `VirtualPool` `[213,224,5,209,98,69,119,92]` |
| On-chain Anchor IDL account | **not published** (`6fSs6jnSMzS2GzLSq2XnhhzY7xeGkJyR2CPYrLL5PB1M` empty). Layout check is live decode, not `anchor idl fetch` |
| Live decode | `state.getPoolConfigs()` → **37 219** configs; `state.getPools()` → **96 819** pools. Sample config `18BLLVHnmv39ykATnxicYFkv4p1emHrpKh2mS1XiuYz` has `quoteMint`, `feeClaimer`, `leftoverReceiver`, `poolFees`, `curve`. Sample pool `11iuqvcUBTL4FrqytcnJVoNgRoEh3mpDK2cLa41BV8n` decodes as `{ poolState: { config, creator, baseMint, quoteMint, … } }` |
| `sdkMatchesDevnet` | **true** |

Official changelog in the 2.0.0 package: SDK 2.0.0 ↔ program 0.2.2 (audited, commit 82054df). `createConfigAndPool*` always go through `createConfig2`. `createConfig` is deprecated.

### Why not 1.5.11 (or 1.5.13)

1.5.11 / 1.5.13 predate program `0.2.2`. They expose `createConfig`, not `createConfig2`, and do not carry the Token-2022 / transfer-fee IDL the deployed program now uses. Falling back would **fail** the discriminator / layout check against today's devnet program.

### Skill conflict

The Meteora Agent Skill pack still pins `1.5.11`. Where `skills/meteora/references/dbc.md` shows 1.5.11 surfaces (`createConfig` only, older quote helpers), follow the **2.0.0** API and https://docs.meteora.ag/developer-guides/dbc/typescript-sdk/reference.

Meteora BUILD rules that still apply: `@solana/web3.js` v1 only (not `@solana/kit` in DBC builders), amounts as `BN` in base units, unsigned txs, no ALTs in messages this contract will parse.

### Build-do-not-send size (same SDK, dummy keys, no ALTs)

`pnpm dbc-tx-size`: `createConfig2` **533** bytes / 7 accounts; `createConfigAndPoolWithFirstBuy` pool+buy **811** bytes / 19 accounts / 7 ixs. Both legacy, both under 1232. `createPoolWithFirstBuy` alone needs an on-chain config; a random live config was RateLimiter, which 2.0.0 correctly refuses for **new** pools. Our curve uses `BaseFeeMode.FeeSchedulerLinear`.

## NEAR SDK pin

**Pinned:** `near-sdk = "=5.17.0"` (toolchain rustc **1.93.0**)

`near-sdk 5.30.0` requires rustc 1.95. 5.17 is current 5.x syntax (`#[near(contract_state)]`, `store::IterableMap`). Transitive `near-sys 0.2.14` needs rustc 1.93. The agent image shipped 1.83.

`cargo-near` was not installed in the agent image. The crate layout matches the `cargo near new` template (cdylib+rlib, overflow-checks on workspace `[profile.release]` / `[profile.dev]`, wasm32). Do not invent a different layout.

Day-1 audit against `.agents/skills/near-contract-audit`: callbacks are `#[private]`; `report_deposit` / `confirm_transfer` take 1 yoctoNEAR; `register` takes 0.005 NEAR and sign requests take 0.01 NEAR (excess refunded); unique map prefixes `t` (trusts), `c` (ledger), `s` (seen sigs), `p` (pending), `k` (locks), `h` (consumed hashes), `n` (lock nonces), `r` (last released); `#[init]` + `PanicOnDefault`; no `Default` impl that skips init.

Milestone 2 (post live test): `create_trust(config)` **requires a watcher** and kicks off `derived_public_key` for `trust/<id>/raise` and `/fees`. `register` / first `report_deposit` derive `trust/<id>/in/<pubkey>` and store `deposit_address`. Sweep/refund **reject** if that address is missing; the transfer source must match it (B1). The ledger is not debited on `sign`: a pending row keyed by `sha256(message)` is locked until the watcher calls `confirm_transfer(message_hash, solana_sig)`. After **150 s** anyone may `release_pending`. A later sign of the same lock increments a per-lock nonce and marks earlier released rows **superseded** (N1). An honest late confirm with no re-sign still books. Settled rows are pruned into `consumed_hashes`. Overflow is refundable in Raising, Filled and Executing. Pending sweeps reduce `refundable` only in Refunding (N2). Swept SOL is refundable from the raise vault in Refunding (`request_raise_refund`). Views and `confirm_transfer` apply timeouts lazily.

**Watcher trust assumption:** NEAR cannot see Solana. The watcher can delay a raise, under/over-report deposits among real contributors, and delay `confirm_transfer`. It cannot change destinations: every signed transfer source and dest is a stored derived key or the contributor's own pubkey. Anyone can run a second watcher to audit reports against explorers. This is the PLAN §4 trust point, restated after R1.

**Toolchain:** rustc stays **1.93.0**. cargo-near wants 1.86 for near-sdk 5.17, but transitive `near-sys 0.2.14` needs 1.93. Size is handled with `wasm-opt -Oz` (`scripts/build-wasm.sh`): Milestone 2 was **251 850 bytes** after -Oz. Milestone 3 Shape C (m7 pin) is **299 355 bytes** after -Oz (532 452 before). Milestone 4 Shape D is **324 604 bytes** after -Oz (573 045 before).

## Milestone 3 — Shape C DBC launch (3 Nov 2026)

User overrides of PLAN.md (follow the user):

| Decision | Why |
| --- | --- |
| `leftoverReceiver` = **raise vault** | Graduation leftover stays in the Trust-owned raise, not the fee vault. `feeClaimer` is still the fee vault. |
| `poolCreator` / first-buy `buyer` = **raise vault**; init `payer` and ATA payer may be the **relayer** | Relayer funds pool-account + ATA rent (~0.0236 SOL on the m6 live run). Raise only spends fill fee + first-buy wrap. |
| Base mint = MPC path `trust/<id>/mint` | No random keypair. Contract derives it at `create_trust` and signs the launch message twice (`Promise::and` raise + mint). |
| First buy = `credited_total − fill_fee` | Fill fee is 1.5% (150 bps) raise → fee vault, in the same tx **or** a prior `request_fill_fee`. `minimum_amount_out ≥ 1`. |
| `request_launch` attach **≥ 90 TGas** | Two 30 TGas `sign` calls + 20 TGas callback. View `launch_attach_tgas()`. |
| `dbc_config` is a 32-byte pubkey | Relayer `createConfig2` (SOL-quoted, Ethics curve scaled down) then `create_trust` pins that pubkey. `params_hash` is still sha256(config\\0name\\0symbol\\0uri). |
| Confirmed launch → `Launched` | Superseded for the confirm method by Milestone 4: `confirm_launch_outcome` records `pool_address`, `fill_fee_paid`, and `tokens_bought`. |
| `request_distribute` panics | Milestone 3 stub. Replaced in Milestone 4. |

Borsh layout adds `mint_path`, `mint_address`, `pool_address`, `fill_fee_paid` on `Trust` and `extra` on `PendingTransfer`. `CreateTrustConfig.cap_bps` is a required `u16` (1..=10_000). Deploy a **new** subaccount after this change (**m7** if m6 is already live); do not reuse m5/m6.

### Live m6 holes (same PR)

| Hole | Fix |
| --- | --- |
| C extra System transfer from raise | Only fee → fee vault and wrap → raise WSOL ATA (`buy` lamports) + SyncNative on that ATA |
| D redirected swap output ATA | Derive and pin all 16 init + 14 swap + ATA/SyncNative/close accounts |
| A raise rent top-up | Relayer is init + ATA payer. The SDK first-buy still sets ATA payer = buyer; `retargetAtaPayer` rewrites those ixs to the relayer. Swap may include `Sysvar1nstructions` when `enableFirstSwapWithMinFee`. |
| B `closeAccount` rejected | Allowed only on raise WSOL ATA, dest + authority = raise |
| Refund rent dust | If leftover would be `0 < x < 890_880`, refund the full remaining deposit (close-out) |
| E `register` after close | Rejected in Launched and Refunding |
| Silent `cap_bps` default | Required field |

### Live m7 holes (same PR)

| Hole | Fix |
| --- | --- |
| F honest SDK launch rejected | Swap pin is the IDL 15-account list. Index 12 = DBC program id (`None` referral). Event @13, program @14, optional sysvar @15. Golden fixture from SDK 2.0.0 builder. |
| G ExactOut swap2 signed | `swap` data is exactly 24 bytes. `swap2` is exactly 25 bytes with `swap_mode = ExactIn (0)`. `minimum_amount_out ≥ 1`. Trailing bytes rejected. |
| H partial refund signed | Refund / raise-refund must equal the full refundable (or dust close-out). |
| Launch lock griefing | `request_launch`, `request_fill_fee`, `request_sweep` are watcher-only. Refunds stay permissionless for liveness (contributors have no bound NEAR identity). A stranger can still lock a full refund for 150 s at 0.01 NEAR — documented leftover, not scaled this PR. |
| Raise still paid init/ATA rent | Init payer and every ATA-create payer must be the tx fee payer (relayer), never raise / mint / fee vault. |

## Milestone 4 — Shape D pro-rata distribution (4 Nov 2026)

User overrides of PLAN §3.5 (follow the user):

| Decision | Why |
| --- | --- |
| Share = `floor(tokens_bought × credited / credited_total)` | Credited is the cap the contributor was given. Swept can lag a confirm. Dust (`tokens_bought − Σ floors`) stays in the raise ATA. It is not paid to a last recipient. Dust is strictly less than the number of credited contributors. |
| `tokens_bought` on `confirm_launch_outcome` only, `U64` JSON string, `1..=1e15` | The contract has no curve params, so it cannot quote the buy. 1e15 is the pinned 1B supply at 6 decimals. Under-report scales every share down (residual stays in the raise ATA). Over-report up to the cap can make transfers fail on Solana if the ATA is short. The watcher cannot change destinations. `confirm_transfer` stays two arguments: near-sdk 5.17 does not serde-default a missing `Option`, and a third field would break sweep / refund JSON. |
| Classic Tokenkeg only | Shape C uses `initialize_virtual_pool_with_spl_token` and rejects Token-2022. Distribution pins TransferChecked decimals 6 and ATA seeds `[owner, Tokenkeg, mint]`. The program id is a constant, not a `Trust` field. |
| Books on confirm, not on sign | Same pending ledger as sweeps: sha256 message, per-contributor lock and nonce, 150 s, `release_pending`, `seen_sigs`, `consumed_hashes`. A re-sign supersedes the released row so release + re-sign cannot double-pay. An honest late confirm with no re-sign still books. |
| Permissionless claim, per-contributor lock | One stranger paying 0.01 NEAR can freeze only the contributors in that message. A batch locks every included leg (at most 5) for 150 s. Contributors left out stay claimable. Launch and sweep stay watcher-only. |
| Deposit 0.01 NEAR, attach ≥ 60 TGas | Same `sign_deposit` as other signs; excess refunded. 60 = 30 TGas `sign` + 20 TGas callback + checker headroom. View `distribute_attach_tgas()`. |
| `Distributed` when `distributed_total >= claimable_total` | `claimable_total` is the sum of floors, not `tokens_bought`. Zero claimable at launch confirm goes straight to `Distributed`. Register is rejected there. `report_deposit` is rejected in Launched and Distributed. |
| Destination = register pubkey | The same owner refunds pay. CreateIdempotent payer is the relayer fee payer, never raise / mint / fee. No extra instructions. |

Borsh adds `tokens_bought`, `claimable_total`, `distributed_total` on `Trust` and `distributed_tokens` on `Contribution`. `PendingTransfer` is unchanged; distribute reuses `extra` as comma-separated `owner_b58:amount:nonce`. No new collection prefixes. Deploy a **new** subaccount (**m9**). Do not reuse m5, m6, or m7.

## MPC `sign` gas and Solana fee payer (3 Nov 2026)

End-to-end on NEAR **testnet** + Solana **devnet**: three MPC-signed System
transfers from vault `6HubCQ3zgHX2mDFgKmPvfVXw1mKUmygkQnyD8S1CLYg`
(path `trust/1/raise`, predecessor `keyless-trust-dev-54ef71.testnet`) to
relayer `BNxpiYoGLU3XRBgZhVu3tToLYJqSvoNo7kaSkKn26FYV`, 0.001 SOL each.
Explorer links: `docs/spikes.md`.

| Decision | Why |
| --- | --- |
| Relayer keypair is the Solana **fee payer** and signs locally | Vault balance dropped by **exactly 1 000 000 lamports** each run. The vault still signs the System transfer as source. |
| Direct `sign` attaches **30 TGas** (`DEFAULT_MPC_SIGN_TGAS` and `mpc.rs` `MPC_SIGN_GAS`) | Burnt gas was **8.84 TGas** each time. 30 is enough headroom. Callbacks still get 20 TGas. |
| No durable nonce | Sign 6.8–7.5 s, send→confirmed ~2.6 s, total ~14 s. The blockhash stayed valid. |
| Devnet submit **works** | Three confirmed transfers. CI still refuses to call `sign` or submit unless `EXECUTE_NEAR_SIGNED_TRANSFER=1` (and `SOLANA_SUBMIT=1` for send). |

On-chain Shapes A / A2 / B accept a relayer fee payer and **require** the stored vault as transfer source. Vault-paid (fee payer = source) still passes.

## Networks

Testnet (NEAR) and Solana **devnet** only. MPC: `v1.signer-prod.testnet`, `domain_id = 1` (Ed25519), `payload_v2.Eddsa`.
