# `keyless-trust` contract

NEAR smart contract for Keyless Trust. Built with near-sdk 5.17
(`#[near(contract_state)]`). Follow `.agents/skills/near-smart-contracts` and
audit against `.agents/skills/near-contract-audit` before expanding it.

Milestone 3 methods: `create_trust(config)` (watcher required; **`cap_bps` is
required**, 1..=10_000; derives raise, fee, and mint paths; `dbc_config` is
the pinned 32-byte DBC config pubkey), `deposit_address`, `register` /
`report_deposit` (`register` is rejected after Launched or Refunding),
`request_sweep`, `request_refund` (full refundable only, including close-out when leftover
would be below 890 880 lamports), `request_raise_refund`, `request_fill_fee`
(watcher-only), `request_sweep` (watcher-only),
`request_launch` (watcher-only; two MPC signs, attach ≥ 90 TGas; the first-buy
quote is local, so that budget is unchanged; relayer must pay init + ATA rent;
swap pin includes Anchor `None` referral = DBC program id; ExactIn;
`minimum_amount_out` must equal the quote), `confirm_launch_outcome`
(message hash + Solana signature only; the basis was stored at launch
request), `confirm_transfer` (sweeps, refunds,
and distribute claims), `request_distribute` (permissionless, Launched only,
one raise MPC sign, attach ≥ 60 TGas, 0.01 NEAR), `release_pending`,
`try_timeout`. Views: `claimable`, `distribution_status`,
`distribute_attach_tgas`.
States: Raising → Filled → Executing → Launched → Distributed, or Refunding.

```bash
cargo test -p keyless-trust
scripts/build-wasm.sh   # release + wasm-opt -Oz when binaryen is installed
# optional: documents the later near-workspaces recipe (ignored in CI)
cargo test -p keyless-trust --test sandbox -- --ignored
```

`register` attaches 0.005 NEAR; sweep/refund/raise-refund/fill-fee/launch/distribute
attach 0.01 NEAR (views `register_deposit` / `sign_deposit`; excess refunded).
Launch needs ≥ 90 TGas (`launch_attach_tgas`: two 30 TGas MPC signs + callback).
Distribute needs ≥ 60 TGas (`distribute_attach_tgas`: one 30 TGas sign + callback
+ headroom). Pending expiry is 150 s. A re-sign supersedes earlier released
hashes for the same lock, including each contributor in a distribute batch.
Wasm after `wasm-opt -Oz` is 339 351 bytes (600 020 before `-Oz`; m9 was
324 604 after `-Oz`). Borsh adds the DBC curve: deploy subaccount **m10**,
not m7 or m9. `request_launch` still attaches 90 TGas; the quote adds no
promise.

Deploy to a subaccount of `keyless-trust-dev-54ef71.testnet` only with
`EXECUTE_DEPLOY=1` and parent creds: `scripts/deploy-testnet.sh`.
Storage deposit is computed from wasm size (floor 5.5 NEAR). No full-access
key is left on the contract unless `DEPLOY_KEEP_KEY=1`. Testnet only.
No secrets in this repo.
