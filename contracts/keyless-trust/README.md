# `keyless-trust` contract

NEAR smart contract for Keyless Trust. Built with near-sdk 5.17
(`#[near(contract_state)]`). Follow `.agents/skills/near-smart-contracts` and
audit against `.agents/skills/near-contract-audit` before expanding it.

Milestone 2 methods: `create_trust(config)` (watcher required; derives raise
and fee vaults), `deposit_address`, `register` / `report_deposit` (derive
deposit address), `request_sweep`, `request_refund`, `request_raise_refund`,
`confirm_transfer`, `release_pending`, `try_timeout`.
States: Raising → Filled → Executing → Launched, or Refunding.

```bash
cargo test -p keyless-trust
scripts/build-wasm.sh   # release + wasm-opt -Oz when binaryen is installed
# optional: documents the later near-workspaces recipe (ignored in CI)
cargo test -p keyless-trust --test sandbox -- --ignored
```

Deploy to a subaccount of `keyless-trust-dev-54ef71.testnet` only with
`EXECUTE_DEPLOY=1` and parent creds: `scripts/deploy-testnet.sh`.
Storage deposit is computed from wasm size (floor 5.5 NEAR). Testnet only.
No secrets in this repo.
