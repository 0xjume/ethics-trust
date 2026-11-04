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
Milestone 2 wasm (this PR, unoptimised): **460 739 bytes**. `wasm-opt -Oz`: **251 850 bytes**. Live run of the previous build used **~221 KB**.

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
unlocks a re-sign. A later sign of the same lock **supersedes** released
hashes (`pending superseded`). An honest late confirm with no re-sign still
books.

### Milestone 2 live rerun (`b20adfe` on m3)

Contract: **`m3.keyless-trust-dev-54ef71.testnet`**. Every flow and must-fail
check passed. Gas **~14.1 TGas** per sign call.

| Action | Link |
| --- | --- |
| Sweep | [3DGKcmod…LPK9fv](https://explorer.solana.com/tx/3DGKcmodF7JFURqz5Cnta3DBUicFCCYj26yxWWvcH6wmeeiidLHXdgfRM1X2g7Twiz8tBkD1j4VNqQ3c27LPK9fv?cluster=devnet) |
| Raise refund | [Fq8EWk3g…LePjKr](https://explorer.solana.com/tx/Fq8EWk3ge3Wy9HxvKDraDSZVQPypC92BC3fRZrNC9KpucLXBqKHFXpQqq7jfjBvVpk9VJcDGqKsE6pGApLePjKr?cluster=devnet) |
| B1 now rejected | [Zkw4EyQv…Mc8W4TY](https://testnet.nearblocks.io/txns/Zkw4EyQvaoBQAxVTUdG3ELX4NoyYX8SYzuecMc8W4TY) |

**N1 (found; budget check was not enough):** a released pending could be
confirmed after a re-sign had already settled the same budget, double-booking
(`refunded=10M` against `overflow=5M`):
[DjhtE5ZURR7ksn3urEgSv4LbZF65AgCEqXY8iTVHVPjh](https://testnet.nearblocks.io/txns/DjhtE5ZURR7ksn3urEgSv4LbZF65AgCEqXY8iTVHVPjh)
(released hash `220b0699…`, reused the re-sign's Solana sig). Sig de-dupe +
remaining-budget check shipped on m4; a **partial** re-sign after a state
change still left room. Closed on this commit: each lock has a nonce; a new
sign supersedes earlier released entries (`pending superseded`).

**N2 (found and fixed):** in Refunding, `refundable` ignored a pending
unconfirmed sweep, so a refund and a sweep of the same lamports could both
be signed. Pending sweeps now reduce `refundable` only in Refunding (same
pot). `sweepable` does not subtract a pending overflow refund.

**N3 (found and fixed):** `register` took 1 yocto and settled pendings were
never pruned. `register` requires **0.005 NEAR**; sign requests require
**0.01 NEAR** (live sign needed ~0.0043; 0.001 failed). Excess is refunded.

Deploy no longer leaves the parent's full-access key on the contract.
`DEPLOY_KEEP_KEY=1` restores the old behaviour.

### Milestone 2 live rerun (`4b5ec17` on m4)

Contract: **`m4.keyless-trust-dev-54ef71.testnet`**. No access keys. N2, N3
and every regression passed. N1 still open: a late confirm of a released
hash with a fake unused `solana_sig` was accepted after a partial re-sign
left leftover budget (nearblocks `AMDGKAuV…`). Sign with exactly
`sign_deposit()` 0.001 NEAR failed (`56VuXGAR…`, needed ~0.0043). Both
closed in this commit. Redeploy as **m5**.

### Milestone 2 live rerun (`6514056` on m5)

Contract: **`m5.keyless-trust-dev-54ef71.testnet`**. Deployed
[B2aH2JZi…](https://testnet.nearblocks.io/txns/B2aH2JZixb5ujV79yK5Xb5Uoj4pPBbgNMGydyjhkwBgU)
(5.5 NEAR, no full-access key; RPC `view_access_key_list` is empty). Storage
**258.05 kB**. 47 transactions.

N4 deposits work on-chain: `register` **0.005 NEAR** and sign requests
**0.01 NEAR**. Late confirms of already-settled or reused-sig hashes fail.
A leftover-budget late confirm is rejected (`amount exceeds remaining budget`).

| Action | Link |
| --- | --- |
| Deploy (FINAL, keyless) | [B2aH2JZi…](https://testnet.nearblocks.io/txns/B2aH2JZixb5ujV79yK5Xb5Uoj4pPBbgNMGydyjhkwBgU) |
| `register` 0.005 | [7JQGFrAU…](https://testnet.nearblocks.io/txns/7JQGFrAU4QU8LSfEh9AwBqLA8671LY2jqzbRQKJC9oLN) |
| Sweep sign 0.01 | [CS3Pf5Sv…](https://testnet.nearblocks.io/txns/CS3Pf5SvMxcaMzQYdWHb5UYENehnArupcxbRNoAxVPgK) |
| Refund sign 0.01 | [8XNzWdq9…](https://testnet.nearblocks.io/txns/8XNzWdq92cbKSQzqLhb2tdiB6N29cAD8YGuVJqxAbhWH) |
| Confirm landed | [2VomgvdQ…](https://testnet.nearblocks.io/txns/2VomgvdQbRV18ESP7E97QU6R48bsUuyyR9c5AWMKHuxH) · Solana [48xKqVuC…](https://explorer.solana.com/tx/48xKqVuCsQ1uxsaRyVDFFj5bXfvYkhV94RnirT2oqeWGwctqZJ1mkPPCA5Bi5zdC72iPSqofjPAXooJfhHmfeRP8?cluster=devnet) |
| Confirm after release | [FfdcXicq…](https://testnet.nearblocks.io/txns/FfdcXicqrhuqJudrQcwZf915kbSmQP7uNhZ5HH31eph6) · Solana [C39F8Yxx…](https://explorer.solana.com/tx/C39F8Yxxqtnfqx2YYc1nSsnKYB3LGgMc7gZiSRM7RaPAw49GREmJpBfhK8VtcZeVMCGTUyv1mdZegNVG28rhBhb?cluster=devnet) |
| Must-fail: already settled | [6cXB4Mp2…](https://testnet.nearblocks.io/txns/6cXB4Mp2mjC8xgBLpfTv3kY69VFywiHEwxN5Pwwysjnb) |
| Must-fail: sig reused | [F4FV1Kqx…](https://testnet.nearblocks.io/txns/F4FV1Kqx1QLh4Qidt5T919H1EH5kevhjgJZYF8K8yf7w) |
| Must-fail: leftover budget | [AMPQ9RrR…](https://testnet.nearblocks.io/txns/AMPQ9RrRPeCzRitaTuCpNpxyUJjQ6XG4j1WLcxQvEQLT) |

Account: [m5.keyless-trust-dev-54ef71.testnet](https://testnet.nearblocks.io/address/m5.keyless-trust-dev-54ef71.testnet).

### Milestone 3 deploy recipe (m7 after the m6 live-test fixes)

Borsh added mint/pool/fill-fee fields and made `cap_bps` required. Deploy a
**new** subaccount; do not reuse m5 or m6. Relayer pays init + ATA rent —
do not send extra SOL to the raise vault. Human-run on testnet + Solana
**devnet** (this agent does not deploy or spend).

```bash
scripts/build-wasm.sh
NEAR_ACCOUNT_ID=keyless-trust-dev-54ef71.testnet \
NEAR_PRIVATE_KEY=ed25519:... \
DEPLOY_SUBACCOUNT=m7 \
EXECUTE_DEPLOY=1 \
pnpm deploy-testnet
# → m7.keyless-trust-dev-54ef71.testnet  (waitUntil FINAL, no full-access key)

export CONTRACT_ID=m7.keyless-trust-dev-54ef71.testnet
export NEAR_PREDECESSOR=$CONTRACT_ID
export TRUST_ID=1

# 1. Derive raise / fees / mint (predecessor = the new contract).
pnpm derive-addresses

# 2. createConfig2 on Solana devnet: feeClaimer=fees, leftoverReceiver=raise.
EXECUTE_CREATE_CONFIG=1 pnpm create-dbc-config
# copy config pubkey → DBC_CONFIG

# 3. create_trust({ dbc_config, name, symbol, uri, watcher, target_lamports, deadline_ns, cap_bps })
#    Wait for derived raise/fees/mint addresses (or request_derived_key kind=mint).

# 4. Register, report_deposit, request_sweep, confirm_transfer until
#    swept_total == credited_total and state == Executing.

# 5. Build the launch message (fill fee in-tx unless FILL_FEE_ALREADY_PAID=1).
CREDITED_LAMPORTS=<credited_total> DBC_CONFIG=<pubkey> \
  pnpm build-launch-message
# copy messageBase64 / messageHash

# 6. Two MPC signs. Attach 90 TGas + 0.01 NEAR.
EXECUTE_REQUEST_LAUNCH=1 LAUNCH_MESSAGE_B64=... pnpm request-launch

# 7. Relayer co-signs and submits (3 signatures: relayer + raise + mint).
RAISE_SIGNATURE=... MINT_SIGNATURE=... \
RAISE_ADDRESS=... MINT_ADDRESS=... \
SOLANA_SUBMIT=1 pnpm relay-launch

# 8. Watcher confirms the launch and the raise-ATA token balance
#    (6-decimal raw units, decimal string). → Launched.
EXECUTE_CONFIRM_LAUNCH=1 LAUNCH_MESSAGE_HASH=... SOLANA_SIG=... \
  TOKENS_BOUGHT=<raw units> \
  pnpm confirm-launch
```

Pro-rata claims are the m9 recipe below. Do not call `request_distribute` on m7.

### Milestone 4 deploy recipe (m9)

Borsh adds `tokens_bought`, `claimable_total`, and `distributed_total` on
`Trust`, and `distributed_tokens` on `Contribution`. Deploy a **new**
subaccount. Do not reuse m5, m6, or m7. This agent does not deploy or spend.
Human-run on NEAR **testnet** and Solana **devnet** only.

```bash
scripts/build-wasm.sh
NEAR_ACCOUNT_ID=keyless-trust-dev-54ef71.testnet \
NEAR_PRIVATE_KEY=ed25519:... \
DEPLOY_SUBACCOUNT=m9 \
EXECUTE_DEPLOY=1 \
pnpm deploy-testnet
# → m9.keyless-trust-dev-54ef71.testnet  (waitUntil FINAL, no full-access key)

export CONTRACT_ID=m9.keyless-trust-dev-54ef71.testnet
export NEAR_PREDECESSOR=$CONTRACT_ID
export TRUST_ID=1

# 1–7. Same raise as the m7 recipe: derive, createConfig2, create_trust,
#      register, sweep, build-launch-message, request-launch (90 TGas),
#      relay-launch. Relayer still pays init + ATA rent.

# 8. Watcher reads the raise token ATA (classic Tokenkeg, mint = trust/<id>/mint,
#    6 decimals) and confirms. tokens_bought is a decimal string (U64).
EXECUTE_CONFIRM_LAUNCH=1 LAUNCH_MESSAGE_HASH=... SOLANA_SIG=... \
  TOKENS_BOUGHT=<raw units> \
  pnpm confirm-launch
# state = Launched, unless every floor share is 0 (then Distributed).

# 9. Build one claim or a batch of up to 5. Amount is the remaining floor
#    share. Omit AMOUNT to compute floor(TOKENS_BOUGHT * CREDITED / TOTAL).
#    CREATE_ATA=0 skips CreateIdempotent when the destination ATA exists.
CONTRIBUTOR_OWNER=<register pubkey> \
TOKENS_BOUGHT=<raw> CREDITED_TOTAL=<credited_total> CONTRIBUTOR_CREDITED=<credited> \
  pnpm build-distribute-message
# or DISTRIBUTE_CLAIMS='[{"owner":"<b58>","amount":"<raw>","createAta":true}]'

# 10. Permissionless. Attach 60 TGas + 0.01 NEAR. One raise MPC sign.
#     Locks only the contributors in the message for 150 s.
EXECUTE_REQUEST_DISTRIBUTE=1 DISTRIBUTE_MESSAGE_B64=... pnpm request-distribute

# 11. Relayer co-signs (2 signatures: relayer + raise) and submits.
RAISE_SIGNATURE=... RAISE_ADDRESS=... \
SOLANA_SUBMIT=1 DISTRIBUTE_MESSAGE_B64=... \
  pnpm relay-distribute

# 12. Watcher confirm_transfer books the shares. Distributed when
#     distributed_total >= claimable_total. Dust stays in the raise ATA.
EXECUTE_CONFIRM_DISTRIBUTE=1 DISTRIBUTE_MESSAGE_HASH=... SOLANA_SIG=... \
  pnpm confirm-distribute
```

### Deploy script

```bash
scripts/build-wasm.sh
NEAR_ACCOUNT_ID=keyless-trust-dev-54ef71.testnet \
NEAR_PRIVATE_KEY=ed25519:... \
DEPLOY_SUBACCOUNT=m6 \
EXECUTE_DEPLOY=1 \
pnpm deploy-testnet
# → m6.keyless-trust-dev-54ef71.testnet
# wrapper: scripts/deploy-testnet.sh
# DEPLOY_NEAR overrides the computed floor (5.5)
# DEPLOY_KEEP_KEY=1 leaves a full-access key (off by default)
# Script waits for the deploy tx to be FINAL before returning
```

## Still needs funded testnet / devnet accounts

- A human-created NEAR testnet account to **deploy** `keyless-trust` (the e2e `sign` used `keyless-trust-dev-54ef71.testnet` as predecessor, not a deployed contract)
- A paid or created DBC **config** (createConfig2) that this Trust pins; dummy keys cannot initialise a pool
- Landing create-and-buy, live sweeps/refunds from the deployed contract, and a near-workspaces gas trace
