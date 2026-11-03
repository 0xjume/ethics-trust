//! Chain Signatures client: `derived_public_key` and `sign` (`payload_v2` / Ed25519).

use near_sdk::serde_json::json;
use near_sdk::{AccountId, Gas, NearToken, Promise};

/// MPC `sign` is yield/resume. The near-smart-contracts skill attaches 250 TGas.
pub const MPC_SIGN_GAS: Gas = Gas::from_tgas(250);
pub const MPC_CALLBACK_GAS: Gas = Gas::from_tgas(20);
pub const DERIVE_GAS: Gas = Gas::from_tgas(15);
pub const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

pub const DOMAIN_ED25519: u64 = 1;

pub fn derive_path(trust_id: u64, kind: &str) -> String {
    format!("trust/{trust_id}/{kind}")
}

pub fn contributor_path(trust_id: u64, solana_pubkey: &str) -> String {
    format!("trust/{trust_id}/in/{solana_pubkey}")
}

pub fn derived_public_key_promise(
    mpc_contract: AccountId,
    path: String,
    predecessor: AccountId,
) -> Promise {
    let args = json!({
        "path": path,
        "predecessor": predecessor,
        "domain_id": DOMAIN_ED25519,
    })
    .to_string()
    .into_bytes();

    Promise::new(mpc_contract).function_call(
        "derived_public_key".to_string(),
        args,
        NearToken::from_near(0),
        DERIVE_GAS,
    )
}

/// Wrap a hex-encoded Solana message as `sign({ request: { path, payload_v2: { Eddsa }, domain_id: 1 }})`.
pub fn sign_eddsa_promise(mpc_contract: AccountId, path: String, payload_hex: String) -> Promise {
    let args = json!({
        "request": {
            "path": path,
            "payload_v2": { "Eddsa": payload_hex },
            "domain_id": DOMAIN_ED25519
        }
    })
    .to_string()
    .into_bytes();

    Promise::new(mpc_contract).function_call(
        "sign".to_string(),
        args,
        ONE_YOCTO,
        MPC_SIGN_GAS,
    )
}

/// `ed25519:<base58>` → Solana base58 address (the 32-byte key).
pub fn solana_address_from_near_key(near_key: &str) -> String {
    near_key
        .strip_prefix("ed25519:")
        .unwrap_or(near_key)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_match_plan() {
        assert_eq!(derive_path(1, "raise"), "trust/1/raise");
        assert_eq!(derive_path(1, "fees"), "trust/1/fees");
        assert_eq!(
            contributor_path(1, "Abc123"),
            "trust/1/in/Abc123"
        );
    }

    #[test]
    fn strips_ed25519_prefix() {
        assert_eq!(
            solana_address_from_near_key(
                "ed25519:B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj"
            ),
            "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj"
        );
    }
}
