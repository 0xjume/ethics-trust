# Day-1 spikes

Measured 3 Nov 2026 from this repository. Derivation, DBC, and parser spikes are read-only. Spike (d) is a live NEAR testnet `sign` plus Solana **devnet** submit. No keys in this repo.

## a) Chain Signatures derivation

Command: `pnpm derive-addresses`  
View: `v1.signer-prod.testnet.derived_public_key` `domain_id = 1`  
Predecessor: `example.testnet` (does not need to exist for the view)

| Path | NEAR key | Solana address |
| --- | --- | --- |
| `trust/1/raise` | `ed25519:B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj` | `B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj` |
| `trust/1/fees` | `ed25519:7bjnioap6J85RWuqPn2CEFRJSSMGvm6CGDR5SsxvxfLa` | `7bjnioap6J85RWuqPn2CEFRJSSMGvm6CGDR5SsxvxfLa` |
| `trust/1/in/B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj` | `ed25519:F1aZA8xz3ueSpV5CQFB8PGz8y5QXZZtFdZJRTcZQ8DR6` | `F1aZA8xz3ueSpV5CQFB8PGz8y5QXZZtFdZJRTcZQ8DR6` |

Plan check: predecessor `example.testnet` + path `trust/1/raise` returned exactly `ed25519:B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj`.

## b) DBC create-and-buy size + SDK vs devnet

Commands: `pnpm verify-dbc-sdk`, `pnpm dbc-tx-size`  
SDK: `@meteora-ag/dynamic-bonding-curve-sdk@2.0.0` (newest npm `latest`; IDL `metadata.version` 0.2.2)  
Related: `@meteora-ag/cp-amm-sdk@1.5.1`  
Program: `dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN`

**Devnet program deployed: yes.** Owner `BPFLoaderUpgradeab1e11111111111111111111111`, 211 141 440 lamports. DAMM v2 `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG` also deployed. Evidence: `docs/DECISIONS.md`.

| Tx | Serialized message (B) | Accounts | Ixs | Fits 1232? |
| --- | --- | --- | --- | --- |
| createConfig2 | 533 | 7 | 1 | yes (699 free) |
| createConfigAndPoolWithFirstBuy.config | 533 | 7 | 1 | yes (699 free) |
| createConfigAndPoolWithFirstBuy.pool+buy | 811 | 19 | 7 | yes (421 free) |

`createPoolWithFirstBuy` alone needs an on-chain config. Dummy keys fail that fetch. A random live config was `BaseFeeMode.RateLimiter`, which SDK 2.0.0 refuses for **new** pools (changelog: deprecated). The combo builder always uses `createConfig2` + our `FeeSchedulerLinear` demo curve. All messages **legacy**, no ALTs.

Live decode (same script): 37 219 `PoolConfig` accounts and 96 819 `VirtualPool` accounts decoded with the 2.0.0 coder. On-chain Anchor IDL account is unpublished; match is IDL-in-SDK + live layout.

## c) Legacy message parser

`cargo test -p keyless-trust` — **17 passed**.

Host-side `env::used_gas()` after parse + Shape A check: **264 768 111 gas** (~0.26 TGas; `parser_gas_estimate_tgas=0` because the mock reports less than 1 TGas). This is the near-sdk mock runtime, not wasm / near-workspaces. A sandbox gas trace still needs a funded or local near-sandbox binary.

Release wasm (day-1): `target/wasm32-unknown-unknown/release/keyless_trust.wasm` **322 671 bytes**.
Milestone 2 wasm (this PR, unoptimised): **456 826 bytes**. `wasm-opt -Oz`: **249 221 bytes**. Live run of the previous build used **~221 KB**.

## d) NEAR-signed Solana transfer (end to end)

Script: `relayer/src/near-signed-solana-transfer.ts`  
MPC: `v1.signer-prod.testnet` `sign`, `domain_id = 1`  
NEAR signer: `keyless-trust-dev-54ef71.testnet`  
Path: `trust/1/raise` → vault `6HubCQ3zgHX2mDFgKmPvfVXw1mKUmygkQnyD8S1CLYg`  
Relayer fee payer: `BNxpiYoGLU3XRBgZhVu3tToLYJqSvoNo7kaSkKn26FYV`  
Amount: **0.001 SOL** (1 000 000 lamports) each, relayer pays the Solana fee, **30 TGas** attached

Three live runs (3 Nov 2026). Every run burnt **8.84 TGas**. Time to signature **6.8–7.5 s**, send → confirmed **~2.6 s**, total **~14 s**. The vault balance dropped by **exactly 1 000 000 lamports** each time. Durable nonces are not needed.

Vault funding (devnet): [`452rHV4vsvxXjAYKXceyeQYmiavoLfAtBbj14kWi9Xdr6S6NCWbRtN9a6WsCi5NLprEiBSeBXdNLhVQKezo6pr9e`](https://explorer.solana.com/tx/452rHV4vsvxXjAYKXceyeQYmiavoLfAtBbj14kWi9Xdr6S6NCWbRtN9a6WsCi5NLprEiBSeBXdNLhVQKezo6pr9e?cluster=devnet)

| # | NEAR `sign` | Solana transfer |
| --- | --- | --- |
| 1 | [`BnwGYCZTcZ41ntCUKEeRMmpwhXeuqxzRfyunSvJfDa4R`](https://testnet.nearblocks.io/txns/BnwGYCZTcZ41ntCUKEeRMmpwhXeuqxzRfyunSvJfDa4R) | [`32cEnng3…Ngbd9G`](https://explorer.solana.com/tx/32cEnng3NKyCv2VLQ47M2uXnVDp1LZH7B7rtWZtzHDP2QHz6yrEbFJw7kyWerMs1CYZ2vMoa3cgrcKmKkGNgbd9G?cluster=devnet) |
| 2 | [`J368XK8TPVruwmy2a1a8UP5bBXKBjZMmLtwi3UKXW1eX`](https://testnet.nearblocks.io/txns/J368XK8TPVruwmy2a1a8UP5bBXKBjZMmLtwi3UKXW1eX) | [`2wBCwpDi…jVTMza`](https://explorer.solana.com/tx/2wBCwpDikerJ7WiYNu4pRGkyP2QTD5EvHg6mFgnvyw2Q291M72Lpj542ZXnAcxewghaGo14cEg6KHiRZKjtVTMza?cluster=devnet) |
| 3 | [`ARkmp9KQfJuoyjqWaKQbopSELQtRkW1vQ9nGj3ko2tGo`](https://testnet.nearblocks.io/txns/ARkmp9KQfJuoyjqWaKQbopSELQtRkW1vQ9nGj3ko2tGo) | [`2UwjXHH8…XEPWr8A`](https://explorer.solana.com/tx/2UwjXHH8ochhUvGhnDx7pSYDwJWUBdsLkav6yEShNXjqWNqMafUTqvU8fu23QEUKWErME8bqY42cvQoU9XEPWr8A?cluster=devnet) |

Full Solana signatures:

- `32cEnng3NKyCv2VLQ47M2uXnVDp1LZH7B7rtWZtzHDP2QHz6yrEbFJw7kyWerMs1CYZ2vMoa3cgrcKmKkGNgbd9G`
- `2wBCwpDikerJ7WiYNu4pRGkyP2QTD5EvHg6mFgnvyw2Q291M72Lpj542ZXnAcxewghaGo14cEg6KHiRZKjtVTMza`
- `2UwjXHH8ochhUvGhnDx7pSYDwJWUBdsLkav6yEShNXjqWNqMafUTqvU8fu23QEUKWErME8bqY42cvQoU9XEPWr8A`

The script uses a **local relayer keypair as Solana fee payer** (vault still signs the transfer) and attaches `DEFAULT_MPC_SIGN_TGAS = 30`. CI never runs the MPC call (`EXECUTE_NEAR_SIGNED_TRANSFER` unset → `{ skipped: true }`).

## e) Milestone 2 contract (chip-in / refund / sweep)

`cargo test -p keyless-trust` — unit tests for the state machine, 1% cap math,
overflow, refund eligibility, relayer-paid Shape A/B, and attacker destinations.

MPC `sign` on the contract path now attaches **30 TGas** (`mpc.rs`), matching
the live burn of **8.84 TGas**. Callback 20 TGas.

`pnpm deposit-address` (read-only) calls `v1.signer-prod.testnet.derived_public_key`
for `trust/<id>/in/<pubkey>`. When `CONTRACT_ID` is set it uses the on-chain
`deposit_address` view first.

near-workspaces is **not** a default-CI dep (no near-sandbox download). The
practical MPC mock is the unit-test callbacks. See
`contracts/keyless-trust/tests/sandbox.rs` for the later workspaces recipe.

### Milestone 2 live run

Contract: **`m2.keyless-trust-dev-54ef71.testnet`** (NEARtestnet). Sweep, overflow
refund, and tampered-destination rejection all worked. Gas **~13.8 TGas** per
sign call (**8.18 TGas** of it MPC), **~8 s** to signature, **~4 s** to confirm.

| Action | Link |
| --- | --- |
| Sweep | [51DxJTpR…8zaRcg](https://explorer.solana.com/tx/51DxJTpRZFyVPgndpeJ4DkK9PhZt5pJuQazqNzhuGbiEdHhLecVnZSgwHvHSejvYmWU3zhJKJoBHt3kYsw8zaRcg?cluster=devnet) |
| Overflow refund | [5wGFPo3e…oyfJtn](https://explorer.solana.com/tx/5wGFPo3erHFEU7sfMC8eYDUEvphyvjbkee93Lfr8Hxz2HjUFt4DQ3JsFH2L8JHiWecpUSEntxPpCicWU3CoyfJtn?cluster=devnet) |
| Tampered dest rejected | [6vDKJvPH…uik4t](https://testnet.nearblocks.io/txns/6vDKJvPHEoHn9qZNzvx5CmX6gdFn6SfsJ5CscY9uik4t) |

**B1 (found and fixed):** transfer source was not enforced because
`deposit_address` was never stored. A refund whose source was the relayer
wallet was accepted and 0.001 SOL was booked as refunded while the SOL stayed
in the deposit address:
[4jqRrYokTwtB3gDDkGKeJvu3WCtaBi4YyxFSmckYx6ez](https://testnet.nearblocks.io/txns/4jqRrYokTwtB3gDDkGKeJvu3WCtaBi4YyxFSmckYx6ez).
Fix: derive+store deposit / raise / fee keys via MPC callback; reject if
missing; source must match the stored deposit (or raise vault for
`request_raise_refund`).

Redeploy after this fix with `scripts/build-wasm.sh` + `pnpm deploy-testnet`
(storage deposit ≥ 5.5 NEAR, wasm-opt -Oz). Wait for `raise_address` /
`deposit_address` before sweep/refund, then `confirm_transfer` after Solana
lands. `release_pending` after ~150s (past the Solana blockhash lifetime)
unlocks a re-sign; a late confirm of the released hash still books only if
the budget is free and the Solana sig is new.

### Milestone 2 live rerun (`b20adfe` on m3)

Contract: **`m3.keyless-trust-dev-54ef71.testnet`**. Every flow and must-fail
check passed. Gas **~14.1 TGas** per sign call.

| Action | Link |
| --- | --- |
| Sweep | [3DGKcmod…LPK9fv](https://explorer.solana.com/tx/3DGKcmodF7JFURqz5Cnta3DBUicFCCYj26yxWWvcH6wmeeiidLHXdgfRM1X2g7Twiz8tBkD1j4VNqQ3c27LPK9fv?cluster=devnet) |
| Raise refund | [Fq8EWk3g…LePjKr](https://explorer.solana.com/tx/Fq8EWk3ge3Wy9HxvKDraDSZVQPypC92BC3fRZrNC9KpucLXBqKHFXpQqq7jfjBvVpk9VJcDGqKsE6pGApLePjKr?cluster=devnet) |
| B1 now rejected | [Zkw4EyQv…Mc8W4TY](https://testnet.nearblocks.io/txns/Zkw4EyQvaoBQAxVTUdG3ELX4NoyYX8SYzuecMc8W4TY) |

**N1 (found and fixed):** a released pending could be confirmed after a re-sign
had already settled the same budget, double-booking (`refunded=10M` against
`overflow=5M`):
[DjhtE5ZURR7ksn3urEgSv4LbZF65AgCEqXY8iTVHVPjh](https://testnet.nearblocks.io/txns/DjhtE5ZURR7ksn3urEgSv4LbZF65AgCEqXY8iTVHVPjh)
(released hash `220b0699…`, reused the re-sign's Solana sig). Fix: confirmed
`solana_sig`s go in the seen set; a released confirm must fit the remaining
budget; `PENDING_EXPIRY` is **150 s** so release is past the Solana blockhash
lifetime (~150 slots / 60–90 s). Settled rows are pruned (hash kept in
`consumed_hashes`).

**N2 (found and fixed):** in Refunding, `refundable` ignored a pending
unconfirmed sweep, so a refund and a sweep of the same lamports could both
be signed. Pending sweeps (and refunds) now reduce the other view.

**N3 (found and fixed):** `register` took 1 yocto and settled pendings were
never pruned. `register` now requires **0.005 NEAR** (`register_deposit`);
sign requests require **0.001 NEAR** (`sign_deposit`); excess is refunded.
Settled pendings are pruned.

Deploy no longer leaves the parent's full-access key on the contract.
`DEPLOY_KEEP_KEY=1` restores the old behaviour.

### Deploy script

```bash
scripts/build-wasm.sh
NEAR_ACCOUNT_ID=keyless-trust-dev-54ef71.testnet \
NEAR_PRIVATE_KEY=ed25519:... \
DEPLOY_SUBACCOUNT=m4 \
EXECUTE_DEPLOY=1 \
pnpm deploy-testnet
# → m4.keyless-trust-dev-54ef71.testnet
# wrapper: scripts/deploy-testnet.sh
# DEPLOY_NEAR overrides the computed floor (5.5)
# DEPLOY_KEEP_KEY=1 leaves a full-access key (off by default)
```

## Still needs funded testnet / devnet accounts

- A human-created NEAR testnet account to **deploy** `keyless-trust` (the e2e `sign` used `keyless-trust-dev-54ef71.testnet` as predecessor, not a deployed contract)
- A paid or created DBC **config** (createConfig2) that this Trust pins; dummy keys cannot initialise a pool
- Landing create-and-buy, live sweeps/refunds from the deployed contract, and a near-workspaces gas trace
