# `keyless-trust` contract

NEAR smart contract for Keyless Trust. Built with near-sdk 5.17
(`#[near(contract_state)]`). Follow `.agents/skills/near-smart-contracts` and
audit against `.agents/skills/near-contract-audit` before expanding it.

Milestone 2 methods: `create_trust(config)`, `deposit_address`,
`report_deposit`, `request_sweep`, `request_refund`, `try_timeout`.
States: Raising → Filled → Executing → Launched, or Refunding.

```bash
cargo test -p keyless-trust
cargo build -p keyless-trust --target wasm32-unknown-unknown --release
# optional: documents the later near-workspaces recipe (ignored in CI)
cargo test -p keyless-trust --test sandbox -- --ignored
```

Deploy to a subaccount of `keyless-trust-dev-54ef71.testnet` only with
`EXECUTE_DEPLOY=1` and parent creds: `scripts/deploy-testnet.sh`.
Testnet only. No secrets in this repo.
