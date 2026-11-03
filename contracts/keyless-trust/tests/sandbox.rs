//! near-workspaces sandbox notes for chip-in / refund / sweep.
//!
//! Default CI does not download `near-sandbox`. The contract unit tests mock
//! the MPC signer by calling the `#[private]` callbacks
//! (`on_derived_public_key`, `on_signature_received`) with canned
//! `ed25519:<pubkey>` / `PromiseError::Failed` results — that is the
//! practical mock for this milestone.
//!
//! To run a real sandbox later:
//! 1. Add `near-workspaces` + `tokio` as dev-dependencies.
//! 2. `cargo build -p keyless-trust --target wasm32-unknown-unknown --release`
//! 3. Deploy the wasm with `worker.dev_deploy`.
//! 4. Deploy a tiny mock MPC account whose `derived_public_key` returns
//!    `ed25519:B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj` and whose `sign`
//!    returns `{ "Ed25519": { "signature": [0u8; 64] } }`.
//! 5. Call `create_trust` / `report_deposit` / `request_sweep` / `request_refund`.
//!
//! Command once those deps exist:
//!   cargo test -p keyless-trust --test sandbox -- --ignored --nocapture

#[test]
#[ignore]
fn mpc_is_mocked_in_unit_tests() {
    // `request_sweep_accepts_relayer_paid_raise_transfer` and
    // `rollback_restores_swept_on_failed_sign` in lib.rs are the mock.
}
