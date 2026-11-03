# `keyless-trust` contract

NEAR smart contract for Keyless Trust. Built with near-sdk 5.17
(`#[near(contract_state)]`). Follow `.agents/skills/near-smart-contracts` and
audit against `.agents/skills/near-contract-audit` before expanding it.

Milestone 3 methods: `create_trust(config)` (watcher required; **`cap_bps` is
required**, 1..=10_000; derives raise, fee, and mint paths; `dbc_config` is
the pinned 32-byte DBC config pubkey), `deposit_address`, `register` /
`report_deposit` (`register` is rejected after Launched or Refunding),
`request_sweep`, `request_refund` (full close-out when leftover would be
below 890 880 lamports), `request_raise_refund`, `request_fill_fee`,
`request_launch` (two MPC signs, attach ≥ 90 TGas; relayer pays init + ATA
rent; every DBC/ATA account is derived and pinned), `confirm_transfer`,
`release_pending`, `try_timeout`. `request_distribute` is a Milestone 4 stub.
States: Raising → Filled → Executing → Launched, or Refunding.

```bash
cargo test -p keyless-trust
scripts/build-wasm.sh   # release + wasm-opt -Oz when binaryen is installed
# optional: documents the later near-workspaces recipe (ignored in CI)
cargo test -p keyless-trust --test sandbox -- --ignored
```

`register` attaches 0.005 NEAR; sweep/refund/raise-refund/fill-fee/launch
attach 0.01 NEAR (views `register_deposit` / `sign_deposit`). Launch also
needs ≥ 90 TGas (`launch_attach_tgas`: two 30 TGas MPC signs + callback).
Pending expiry is 150 s. A re-sign supersedes earlier released hashes for
the same lock. Wasm after `wasm-opt -Oz` is 298 962 bytes.

Deploy to a subaccount of `keyless-trust-dev-54ef71.testnet` only with
`EXECUTE_DEPLOY=1` and parent creds: `scripts/deploy-testnet.sh`.
Storage deposit is computed from wasm size (floor 5.5 NEAR). No full-access
key is left on the contract unless `DEPLOY_KEEP_KEY=1`. Testnet only.
No secrets in this repo.
