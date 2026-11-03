# Day-1 spikes

Measured 3 Nov 2026 from this repository. Testnet / devnet **reads** only. No accounts created, no faucets, no deploys, no send.

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

Release wasm: `target/wasm32-unknown-unknown/release/keyless_trust.wasm` **322 671 bytes**.

## d) NEAR-signed Solana transfer

Script: `relayer/src/near-signed-solana-transfer.ts`  
**Not executed.** `EXECUTE_NEAR_SIGNED_TRANSFER` unset → script prints `{ skipped: true }` and exits 0. Needs `NEAR_ACCOUNT_ID`, `NEAR_PRIVATE_KEY`, a predecessor that owns the path, and SOL on the derived vault.

## Still needs funded testnet / devnet accounts

- A human-created NEAR testnet account to deploy `keyless-trust` (this environment must not create or fund one)
- 1 yoctoNEAR + ~250 TGas to call `sign` on `v1.signer-prod.testnet`
- SOL on a derived vault so a signed System transfer can land
- A paid or created DBC **config** (createConfig2) that this Trust pins; dummy keys cannot initialise a pool
- Landing create-and-buy, sweeps, refunds, and a near-workspaces gas trace
