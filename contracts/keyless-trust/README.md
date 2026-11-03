# `keyless-trust` contract

NEAR smart contract for Keyless Trust. Built with near-sdk 5.17
(`#[near(contract_state)]`). Follow `.agents/skills/near-smart-contracts` and
audit against `.agents/skills/near-contract-audit` before expanding it.

```bash
cargo test -p keyless-trust
cargo build -p keyless-trust --target wasm32-unknown-unknown --release
```

Do not deploy from this environment. Testnet only, after a human creates the
account.
