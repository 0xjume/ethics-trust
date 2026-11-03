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

**Toolchain:** rustc stays **1.93.0**. cargo-near wants 1.86 for near-sdk 5.17, but transitive `near-sys 0.2.14` needs 1.93. Size is handled with `wasm-opt -Oz` (`scripts/build-wasm.sh`): this PR is **251 850 bytes** after -Oz (460 739 before). The previous live deploy measured ~221 KB.

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
