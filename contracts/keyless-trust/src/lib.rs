//! Keyless Trust — NEAR contract that will only sign allowed Solana vault txs.
//!
//! Day-1 scaffold: registry, state machine, views, SystemProgram transfer
//! shape check, and Chain Signatures promise wrappers. No deploy in this PR.

use near_sdk::store::IterableMap;
use near_sdk::json_types::Base64VecU8;
use near_sdk::serde_json::Value;
use near_sdk::{
    env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError,
};

mod mpc;
pub mod shapes;
pub mod solana;

use mpc::{
    contributor_path, derive_path, derived_public_key_promise, sign_eddsa_promise, DOMAIN_ED25519,
    MPC_CALLBACK_GAS, ONE_YOCTO,
};
use shapes::{validate_system_transfer, TransferCheck};
use solana::parse_legacy_message;

const DEFAULT_MPC: &str = "v1.signer-prod.testnet";
const DEFAULT_DBC: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
const DEFAULT_CAP_BPS: u16 = 100;
const STORAGE_PREFIX_TRUSTS: &[u8] = b"t";

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TrustState {
    Open,
    Executing,
    Launched,
    Distributing,
    Distributed,
    Refunding,
    Closed,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Trust {
    pub id: u64,
    pub target_lamports: u64,
    pub cap_bps: u16,
    pub deadline_ns: u64,
    pub raise_path: String,
    pub fee_path: String,
    pub raise_address: Option<String>,
    pub fee_address: Option<String>,
    pub dbc_config: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub state: TrustState,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct TrustView {
    pub id: u64,
    pub target_lamports: u64,
    pub cap_bps: u16,
    pub deadline_ns: u64,
    pub raise_path: String,
    pub fee_path: String,
    pub raise_address: Option<String>,
    pub fee_address: Option<String>,
    pub dbc_config: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub state: TrustState,
}

impl From<&Trust> for TrustView {
    fn from(trust: &Trust) -> Self {
        Self {
            id: trust.id,
            target_lamports: trust.target_lamports,
            cap_bps: trust.cap_bps,
            deadline_ns: trust.deadline_ns,
            raise_path: trust.raise_path.clone(),
            fee_path: trust.fee_path.clone(),
            raise_address: trust.raise_address.clone(),
            fee_address: trust.fee_address.clone(),
            dbc_config: trust.dbc_config.clone(),
            name: trust.name.clone(),
            symbol: trust.symbol.clone(),
            uri: trust.uri.clone(),
            state: trust.state.clone(),
        }
    }
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    mpc_contract: AccountId,
    dbc_program: String,
    next_trust_id: u64,
    trusts: IterableMap<u64, Trust>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(mpc_contract: Option<AccountId>, dbc_program: Option<String>) -> Self {
        Self {
            mpc_contract: mpc_contract.unwrap_or_else(|| DEFAULT_MPC.parse().unwrap()),
            dbc_program: dbc_program.unwrap_or_else(|| DEFAULT_DBC.to_string()),
            next_trust_id: 1,
            trusts: IterableMap::new(STORAGE_PREFIX_TRUSTS),
        }
    }

    pub fn mpc_contract(&self) -> AccountId {
        self.mpc_contract.clone()
    }

    pub fn dbc_program(&self) -> String {
        self.dbc_program.clone()
    }

    pub fn get_trust(&self, trust_id: u64) -> Option<TrustView> {
        self.trusts.get(&trust_id).map(TrustView::from)
    }

    pub fn list_trusts(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<TrustView> {
        let skip = from_index.unwrap_or(0) as usize;
        let take = limit.unwrap_or(20).min(50) as usize;
        self.trusts
            .values()
            .skip(skip)
            .take(take)
            .map(TrustView::from)
            .collect()
    }

    pub fn derivation_paths(&self, trust_id: u64, solana_pubkey: Option<String>) -> Vec<String> {
        let mut paths = vec![derive_path(trust_id, "raise"), derive_path(trust_id, "fees")];
        if let Some(pk) = solana_pubkey {
            paths.push(contributor_path(trust_id, &pk));
        }
        paths
    }

    /// Permissionless create. Parameters are immutable after this call.
    #[payable]
    pub fn create_trust(
        &mut self,
        target_lamports: u64,
        deadline_ns: u64,
        dbc_config: String,
        name: String,
        symbol: String,
        uri: String,
        cap_bps: Option<u16>,
    ) -> u64 {
        require!(
            env::attached_deposit() >= NearToken::from_millinear(10),
            "attach at least 0.01 NEAR for storage"
        );
        require!(target_lamports > 0, "target_lamports must be > 0");
        require!(!dbc_config.is_empty(), "dbc_config is required");
        require!(!name.is_empty() && !symbol.is_empty(), "name and symbol required");
        require!(uri.len() <= 60, "uri must be short (<= 60) for the DBC size budget");

        let id = self.next_trust_id;
        self.next_trust_id += 1;
        let cap = cap_bps.unwrap_or(DEFAULT_CAP_BPS);
        require!(cap > 0 && cap <= 10_000, "cap_bps out of range");

        let trust = Trust {
            id,
            target_lamports,
            cap_bps: cap,
            deadline_ns,
            raise_path: derive_path(id, "raise"),
            fee_path: derive_path(id, "fees"),
            raise_address: None,
            fee_address: None,
            dbc_config,
            name,
            symbol,
            uri,
            state: TrustState::Open,
        };
        self.trusts.insert(id, trust);

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.1.0\",\"event\":\"trust_created\",\"data\":[{{\"trust_id\":{id}}}]}}"
        ));
        id
    }

    /// Anyone may register any Solana pubkey. Refunds can only go to that pubkey.
    #[payable]
    pub fn register(&mut self, trust_id: u64, solana_pubkey: String) -> String {
        require!(
            env::attached_deposit() >= NearToken::from_yoctonear(1),
            "attach 1 yoctoNEAR"
        );
        require!(self.trusts.contains_key(&trust_id), "unknown trust");
        require!(
            solana_pubkey.len() >= 32 && solana_pubkey.len() <= 44,
            "solana_pubkey looks invalid"
        );
        let path = contributor_path(trust_id, &solana_pubkey);
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.1.0\",\"event\":\"registered\",\"data\":[{{\"trust_id\":{trust_id},\"path\":\"{path}\"}}]}}"
        ));
        path
    }

    /// Ask the MPC signer for the derived Ed25519 key of a Trust path.
    pub fn request_derived_key(&self, trust_id: u64, kind: String) -> Promise {
        require!(self.trusts.contains_key(&trust_id), "unknown trust");
        require!(kind == "raise" || kind == "fees", "kind must be raise or fees");
        let path = derive_path(trust_id, &kind);
        derived_public_key_promise(
            self.mpc_contract.clone(),
            path,
            env::current_account_id(),
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(Gas::from_tgas(10))
                .on_derived_public_key(trust_id, kind),
        )
    }

    /// Validate a refund-shaped System transfer, then request an Ed25519 MPC signature.
    /// Does not send the Solana transaction. Requires 1 yoctoNEAR.
    #[payable]
    pub fn request_refund_sign(
        &mut self,
        trust_id: u64,
        owner: Base64VecU8,
        max_lamports: u64,
        message: Base64VecU8,
    ) -> Promise {
        require!(env::attached_deposit() >= ONE_YOCTO, "attach 1 yoctoNEAR");
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        require!(
            trust.state == TrustState::Open || trust.state == TrustState::Refunding,
            "refunds only in Open or Refunding"
        );
        let dest: [u8; 32] = owner
            .0
            .as_slice()
            .try_into()
            .expect("owner must be a 32-byte Solana pubkey");
        let msg = parse_legacy_message(&message.0).expect("invalid Solana message");
        let checked = validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: dest,
                max_lamports,
            },
        )
        .unwrap_or_else(|e| env::panic_str(e.as_str()));

        let payload_hex = hex::encode(&message.0);
        let message_hash_hex = hex::encode(env::sha256(&message.0));
        let path = contributor_path(trust_id, &bs58_hint(&checked.from));

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.1.0\",\"event\":\"refund_checked\",\"data\":[{{\"trust_id\":{trust_id},\"lamports\":{}}}]}}",
            checked.lamports
        ));

        sign_eddsa_promise(self.mpc_contract.clone(), path, payload_hex).then(
            Self::ext(env::current_account_id())
                .with_static_gas(MPC_CALLBACK_GAS)
                .on_signature_received(trust_id, "refund".to_string(), message_hash_hex),
        )
    }

    pub fn domain_id(&self) -> u64 {
        DOMAIN_ED25519
    }

    #[private]
    pub fn on_derived_public_key(
        &mut self,
        trust_id: u64,
        kind: String,
        #[callback_result] result: Result<String, PromiseError>,
    ) -> Option<String> {
        match result {
            Ok(key) => {
                if let Some(trust) = self.trusts.get_mut(&trust_id) {
                    if kind == "raise" {
                        trust.raise_address = Some(mpc::solana_address_from_near_key(&key));
                    } else if kind == "fees" {
                        trust.fee_address = Some(mpc::solana_address_from_near_key(&key));
                    }
                }
                env::log_str(&format!(
                    "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.1.0\",\"event\":\"derived_key\",\"data\":[{{\"trust_id\":{trust_id},\"kind\":\"{kind}\",\"key\":\"{key}\"}}]}}"
                ));
                Some(key)
            }
            Err(_) => {
                env::log_str("derived_public_key failed");
                None
            }
        }
    }

    #[private]
    pub fn on_signature_received(
        &self,
        trust_id: u64,
        action: String,
        message_hash_hex: String,
        #[callback_result] result: Result<Value, PromiseError>,
    ) -> Option<Value> {
        match result {
            Ok(signature) => {
                env::log_str(&format!(
                    "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.1.0\",\"event\":\"mpc_signature\",\"data\":[{{\"trust_id\":{trust_id},\"action\":\"{action}\",\"message_hash\":\"{message_hash_hex}\"}}]}}"
                ));
                Some(signature)
            }
            Err(_) => {
                env::log_str("MPC signature request failed");
                None
            }
        }
    }
}

/// Placeholder label for logs; the path uses the contributor's base58 address
/// supplied at `register`. The 32-byte source is hex-encoded here so the
/// contract never invents a base58 alphabet in WASM.
fn bs58_hint(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solana::message::{serialize_legacy_message, CompiledInstruction, LegacyMessage, MessageHeader};
    use crate::shapes::SYSTEM_PROGRAM;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    fn context(predecessor: AccountId, deposit: NearToken) -> VMContextBuilder {
        let mut b = VMContextBuilder::new();
        b.current_account_id(accounts(0))
            .predecessor_account_id(predecessor)
            .attached_deposit(deposit);
        b
    }

    fn transfer_bytes(from: [u8; 32], to: [u8; 32], lamports: u64) -> Vec<u8> {
        let mut data = vec![2, 0, 0, 0];
        data.extend_from_slice(&lamports.to_le_bytes());
        serialize_legacy_message(&LegacyMessage {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed: 0,
                num_readonly_unsigned: 1,
            },
            account_keys: vec![from, to, SYSTEM_PROGRAM],
            recent_blockhash: [9u8; 32],
            instructions: vec![CompiledInstruction {
                program_id_index: 2,
                accounts: vec![0, 1],
                data,
            }],
        })
    }

    #[test]
    fn create_and_view_trust() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = c.create_trust(
            2_000_000_000,
            1,
            "Config1111111111111111111111111111111111111".into(),
            "Demo".into(),
            "DEMO".into(),
            "https://example.com/m.json".into(),
            Some(100),
        );
        let trust = c.get_trust(id).unwrap();
        assert_eq!(trust.state, TrustState::Open);
        assert_eq!(trust.raise_path, "trust/1/raise");
        assert_eq!(trust.cap_bps, 100);
        assert_eq!(c.mpc_contract().as_str(), DEFAULT_MPC);
    }

    #[test]
    fn register_returns_contributor_path() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = c.create_trust(
            1,
            1,
            "C".into(),
            "N".into(),
            "S".into(),
            "https://x.com/a".into(),
            None,
        );
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        let path = c.register(id, "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj".into());
        assert_eq!(path, "trust/1/in/B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj");
    }

    #[test]
    #[should_panic(expected = "transfer destination is not the fixed owner")]
    fn request_refund_rejects_wrong_destination() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = c.create_trust(
            1,
            1,
            "C".into(),
            "N".into(),
            "S".into(),
            "https://x.com/a".into(),
            None,
        );
        let owner = [2u8; 32];
        let attacker = [9u8; 32];
        let bytes = transfer_bytes([1u8; 32], attacker, 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund_sign(id, Base64VecU8(owner.to_vec()), 100, Base64VecU8(bytes));
    }
}
