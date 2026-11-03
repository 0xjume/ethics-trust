//! Keyless Trust — NEAR contract that will only sign allowed Solana vault txs.
//!
//! Milestone 2: create_trust, deposit addresses, watcher report_deposit,
//! capped sweep into the raise vault, and contributor-only refunds.

use near_sdk::json_types::Base64VecU8;
use near_sdk::serde_json::Value;
use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{
    env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError,
};

mod bs58;
mod math;
mod mpc;
pub mod shapes;
pub mod solana;

use math::{
    refundable_lamports, split_deposit, sweepable_lamports, DEFAULT_CAP_BPS, DEFAULT_FILL_FEE_BPS,
    EXECUTION_TIMEOUT_NS,
};
use mpc::{
    contributor_path, derive_path, derived_public_key_promise, sign_eddsa_promise, DOMAIN_ED25519,
    MPC_CALLBACK_GAS, ONE_YOCTO,
};
use shapes::{validate_system_transfer, TransferCheck};
use solana::parse_legacy_message;

const DEFAULT_MPC: &str = "v1.signer-prod.testnet";
const DEFAULT_DBC: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
const STORAGE_PREFIX_TRUSTS: &[u8] = b"t";
const STORAGE_PREFIX_LEDGER: &[u8] = b"c";
const STORAGE_PREFIX_SIGS: &[u8] = b"s";

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TrustState {
    Raising,
    Filled,
    Executing,
    Launched,
    Refunding,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct CreateTrustConfig {
    pub target_lamports: u64,
    pub deadline_ns: u64,
    pub dbc_config: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub cap_bps: Option<u16>,
    pub fill_fee_bps: Option<u16>,
    pub execution_timeout_ns: Option<u64>,
    pub watcher: Option<AccountId>,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Trust {
    pub id: u64,
    pub target_lamports: u64,
    pub cap_bps: u16,
    pub fill_fee_bps: u16,
    pub deadline_ns: u64,
    pub execution_timeout_ns: u64,
    pub filled_at_ns: Option<u64>,
    pub execution_started_ns: Option<u64>,
    pub raise_path: String,
    pub fee_path: String,
    pub raise_address: Option<String>,
    pub fee_address: Option<String>,
    pub dbc_config: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub params_hash: String,
    pub credited_total: u64,
    pub overflow_total: u64,
    pub watcher: Option<AccountId>,
    pub state: TrustState,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct TrustView {
    pub id: u64,
    pub target_lamports: u64,
    pub cap_bps: u16,
    pub fill_fee_bps: u16,
    pub deadline_ns: u64,
    pub execution_timeout_ns: u64,
    pub filled_at_ns: Option<u64>,
    pub execution_started_ns: Option<u64>,
    pub raise_path: String,
    pub fee_path: String,
    pub raise_address: Option<String>,
    pub fee_address: Option<String>,
    pub dbc_config: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub params_hash: String,
    pub credited_total: u64,
    pub overflow_total: u64,
    pub watcher: Option<AccountId>,
    pub state: TrustState,
}

impl From<&Trust> for TrustView {
    fn from(trust: &Trust) -> Self {
        Self {
            id: trust.id,
            target_lamports: trust.target_lamports,
            cap_bps: trust.cap_bps,
            fill_fee_bps: trust.fill_fee_bps,
            deadline_ns: trust.deadline_ns,
            execution_timeout_ns: trust.execution_timeout_ns,
            filled_at_ns: trust.filled_at_ns,
            execution_started_ns: trust.execution_started_ns,
            raise_path: trust.raise_path.clone(),
            fee_path: trust.fee_path.clone(),
            raise_address: trust.raise_address.clone(),
            fee_address: trust.fee_address.clone(),
            dbc_config: trust.dbc_config.clone(),
            name: trust.name.clone(),
            symbol: trust.symbol.clone(),
            uri: trust.uri.clone(),
            params_hash: trust.params_hash.clone(),
            credited_total: trust.credited_total,
            overflow_total: trust.overflow_total,
            watcher: trust.watcher.clone(),
            state: trust.state.clone(),
        }
    }
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Contribution {
    pub trust_id: u64,
    pub contributor: String,
    pub deposit_path: String,
    pub deposit_address: Option<String>,
    pub reported_lamports: u64,
    pub credited_lamports: u64,
    pub overflow_lamports: u64,
    pub swept_lamports: u64,
    pub refunded_lamports: u64,
    pub last_solana_sig: String,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct DepositAddressView {
    pub trust_id: u64,
    pub contributor: String,
    pub path: String,
    pub predecessor: AccountId,
    pub mpc_contract: AccountId,
    pub domain_id: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    mpc_contract: AccountId,
    dbc_program: String,
    next_trust_id: u64,
    trusts: IterableMap<u64, Trust>,
    ledger: IterableMap<String, Contribution>,
    seen_sigs: LookupMap<String, u64>,
}

fn ledger_key(trust_id: u64, contributor: &str) -> String {
    format!("{trust_id}:{contributor}")
}

fn pin_params_hash(dbc_config: &str, name: &str, symbol: &str, uri: &str) -> String {
    let mut buf = Vec::with_capacity(dbc_config.len() + name.len() + symbol.len() + uri.len() + 3);
    buf.extend_from_slice(dbc_config.as_bytes());
    buf.push(0);
    buf.extend_from_slice(name.as_bytes());
    buf.push(0);
    buf.extend_from_slice(symbol.as_bytes());
    buf.push(0);
    buf.extend_from_slice(uri.as_bytes());
    hex::encode(env::sha256(&buf))
}

fn require_pubkey(contributor: &str) -> [u8; 32] {
    require!(
        contributor.len() >= 32 && contributor.len() <= 44,
        "solana_pubkey looks invalid"
    );
    bs58::decode_pubkey(contributor).unwrap_or_else(|e| env::panic_str(e))
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
            ledger: IterableMap::new(STORAGE_PREFIX_LEDGER),
            seen_sigs: LookupMap::new(STORAGE_PREFIX_SIGS),
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

    pub fn get_contribution(&self, trust_id: u64, contributor: String) -> Option<Contribution> {
        self.ledger.get(&ledger_key(trust_id, &contributor)).cloned()
    }

    pub fn derivation_paths(&self, trust_id: u64, solana_pubkey: Option<String>) -> Vec<String> {
        let mut paths = vec![derive_path(trust_id, "raise"), derive_path(trust_id, "fees")];
        if let Some(pk) = solana_pubkey {
            paths.push(contributor_path(trust_id, &pk));
        }
        paths
    }

    /// Path `trust/<id>/in/<pubkey>` plus the MPC view inputs. The relayer
    /// calls `v1.signer-prod.testnet.derived_public_key` (domain 1) to turn
    /// this into a Solana address — no curve math in WASM.
    pub fn deposit_address(
        &self,
        trust_id: u64,
        contributor_solana_pubkey: String,
    ) -> DepositAddressView {
        require!(self.trusts.contains_key(&trust_id), "unknown trust");
        let _ = require_pubkey(&contributor_solana_pubkey);
        DepositAddressView {
            trust_id,
            path: contributor_path(trust_id, &contributor_solana_pubkey),
            contributor: contributor_solana_pubkey,
            predecessor: env::current_account_id(),
            mpc_contract: self.mpc_contract.clone(),
            domain_id: DOMAIN_ED25519,
        }
    }

    /// Permissionless create. Parameters (including the DBC/mint hash) are
    /// immutable after this call. Vault paths: `trust/<id>/raise` and `/fees`.
    #[payable]
    pub fn create_trust(&mut self, config: CreateTrustConfig) -> u64 {
        require!(
            env::attached_deposit() >= NearToken::from_millinear(10),
            "attach at least 0.01 NEAR for storage"
        );
        require!(config.target_lamports > 0, "target_lamports must be > 0");
        require!(!config.dbc_config.is_empty(), "dbc_config is required");
        require!(
            !config.name.is_empty() && !config.symbol.is_empty(),
            "name and symbol required"
        );
        require!(
            config.uri.len() <= 60,
            "uri must be short (<= 60) for the DBC size budget"
        );

        let id = self.next_trust_id;
        self.next_trust_id += 1;
        let cap = config.cap_bps.unwrap_or(DEFAULT_CAP_BPS);
        let fill_fee = config.fill_fee_bps.unwrap_or(DEFAULT_FILL_FEE_BPS);
        let timeout = config
            .execution_timeout_ns
            .unwrap_or(EXECUTION_TIMEOUT_NS);
        require!(cap > 0 && cap <= 10_000, "cap_bps out of range");
        require!(fill_fee <= 10_000, "fill_fee_bps out of range");
        require!(timeout > 0, "execution_timeout_ns must be > 0");

        let params_hash = pin_params_hash(&config.dbc_config, &config.name, &config.symbol, &config.uri);
        let trust = Trust {
            id,
            target_lamports: config.target_lamports,
            cap_bps: cap,
            fill_fee_bps: fill_fee,
            deadline_ns: config.deadline_ns,
            execution_timeout_ns: timeout,
            filled_at_ns: None,
            execution_started_ns: None,
            raise_path: derive_path(id, "raise"),
            fee_path: derive_path(id, "fees"),
            raise_address: None,
            fee_address: None,
            dbc_config: config.dbc_config,
            name: config.name,
            symbol: config.symbol,
            uri: config.uri,
            params_hash,
            credited_total: 0,
            overflow_total: 0,
            watcher: config.watcher,
            state: TrustState::Raising,
        };
        self.trusts.insert(id, trust);

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.2.0\",\"event\":\"trust_created\",\"data\":[{{\"trust_id\":{id}}}]}}"
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
        let _ = require_pubkey(&solana_pubkey);
        let path = self.ensure_contribution(trust_id, &solana_pubkey).deposit_path;
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.2.0\",\"event\":\"registered\",\"data\":[{{\"trust_id\":{trust_id},\"path\":\"{path}\"}}]}}"
        ));
        path
    }

    /// Trusted watcher records a Solana deposit. Cap and target are enforced
    /// here; overflow is refundable. Destinations are not taken from the
    /// watcher — every later sign re-derives them on chain.
    #[payable]
    pub fn report_deposit(
        &mut self,
        trust_id: u64,
        contributor: String,
        lamports: u64,
        solana_sig: String,
    ) -> Contribution {
        require!(env::attached_deposit() >= ONE_YOCTO, "attach 1 yoctoNEAR");
        require!(lamports > 0, "lamports must be > 0");
        require!(
            solana_sig.len() >= 64 && solana_sig.len() <= 128,
            "solana_sig looks invalid"
        );
        require!(
            self.seen_sigs.get(&solana_sig).is_none(),
            "solana_sig already reported"
        );
        let _ = require_pubkey(&contributor);
        self.apply_timeouts(trust_id);

        let (cap_bps, target, credited_total, watcher, state) = {
            let trust = self.trusts.get(&trust_id).expect("unknown trust");
            (
                trust.cap_bps,
                trust.target_lamports,
                trust.credited_total,
                trust.watcher.clone(),
                trust.state.clone(),
            )
        };
        if let Some(expected) = watcher {
            require!(
                env::predecessor_account_id() == expected,
                "only the trusted watcher can report deposits"
            );
        }
        require!(
            state != TrustState::Launched,
            "cannot report deposits after launch"
        );

        let mut row = self.ensure_contribution(trust_id, &contributor);
        let split = if state == TrustState::Raising {
            split_deposit(
                lamports,
                row.credited_lamports,
                credited_total,
                target,
                cap_bps,
            )
        } else {
            math::DepositSplit {
                credited: 0,
                overflow: lamports,
            }
        };

        row.reported_lamports = row.reported_lamports.saturating_add(lamports);
        row.credited_lamports = row.credited_lamports.saturating_add(split.credited);
        row.overflow_lamports = row.overflow_lamports.saturating_add(split.overflow);
        row.last_solana_sig = solana_sig.clone();
        let out = row.clone();
        self.ledger.insert(ledger_key(trust_id, &contributor), row);
        self.seen_sigs.insert(solana_sig, trust_id);

        if let Some(trust) = self.trusts.get_mut(&trust_id) {
            trust.credited_total = trust.credited_total.saturating_add(split.credited);
            trust.overflow_total = trust.overflow_total.saturating_add(split.overflow);
            if trust.state == TrustState::Raising && trust.credited_total >= trust.target_lamports {
                trust.state = TrustState::Filled;
                trust.filled_at_ns = Some(env::block_timestamp());
            }
        }
        self.apply_timeouts(trust_id);

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.2.0\",\"event\":\"deposit_reported\",\"data\":[{{\"trust_id\":{trust_id},\"credited\":{},\"overflow\":{}}}]}}",
            split.credited, split.overflow
        ));
        out
    }

    /// Move a filled raise from Filled/Executing into Refunding when the
    /// deadline or 24h execution timeout has passed. Permissionless.
    pub fn try_timeout(&mut self, trust_id: u64) -> TrustState {
        self.apply_timeouts(trust_id);
        self.trusts
            .get(&trust_id)
            .expect("unknown trust")
            .state
            .clone()
    }

    pub fn refundable(&self, trust_id: u64, contributor: String) -> u64 {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        match self.ledger.get(&ledger_key(trust_id, &contributor)) {
            Some(row) => refundable_lamports(
                &trust.state,
                row.credited_lamports,
                row.overflow_lamports,
                row.swept_lamports,
                row.refunded_lamports,
            ),
            None => 0,
        }
    }

    pub fn fill_fee(&self, trust_id: u64) -> u64 {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        math::fill_fee_lamports(trust.credited_total, trust.fill_fee_bps)
    }

    pub fn sweepable(&self, trust_id: u64, contributor: String) -> u64 {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        match self.ledger.get(&ledger_key(trust_id, &contributor)) {
            Some(row) => sweepable_lamports(&trust.state, row.credited_lamports, row.swept_lamports),
            None => 0,
        }
    }

    /// Ask the MPC signer for the derived Ed25519 key of a Trust path.
    pub fn request_derived_key(&self, trust_id: u64, kind: String) -> Promise {
        require!(self.trusts.contains_key(&trust_id), "unknown trust");
        require!(kind == "raise" || kind == "fees", "kind must be raise or fees");
        let path = derive_path(trust_id, &kind);
        derived_public_key_promise(self.mpc_contract.clone(), path, env::current_account_id()).then(
            Self::ext(env::current_account_id())
                .with_static_gas(Gas::from_tgas(10))
                .on_derived_public_key(trust_id, kind),
        )
    }

    /// Validate a relayer-paid (or vault-paid) sweep into the raise vault,
    /// cap so credited never exceeds 1% of target, then MPC-sign (Ed25519,
    /// domain 1, 1 yocto, ~30 TGas). Destination is re-derived from
    /// `trust/<id>/raise` — the caller cannot pick one.
    #[payable]
    pub fn request_sweep(
        &mut self,
        trust_id: u64,
        contributor: String,
        message: Base64VecU8,
    ) -> Promise {
        require!(env::attached_deposit() >= ONE_YOCTO, "attach 1 yoctoNEAR");
        self.apply_timeouts(trust_id);
        let dest = self.raise_dest(trust_id);
        let max = self.sweepable(trust_id, contributor.clone());
        require!(max > 0, "nothing to sweep");
        let source = self.optional_deposit_source(trust_id, &contributor);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        self.book_sweep(trust_id, &contributor, checked.lamports);
        self.sign_vault_tx(trust_id, &contributor, "sweep", checked.lamports, &message.0)
    }

    /// Raising: refund uncredited overflow only. Refunding: everything not
    /// already swept. Destination is only the contributor's own pubkey.
    #[payable]
    pub fn request_refund(
        &mut self,
        trust_id: u64,
        contributor: String,
        message: Base64VecU8,
    ) -> Promise {
        require!(env::attached_deposit() >= ONE_YOCTO, "attach 1 yoctoNEAR");
        self.apply_timeouts(trust_id);
        let dest = require_pubkey(&contributor);
        let max = self.refundable(trust_id, contributor.clone());
        require!(max > 0, "nothing to refund");
        let source = self.optional_deposit_source(trust_id, &contributor);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        self.book_refund(trust_id, &contributor, checked.lamports);
        self.sign_vault_tx(trust_id, &contributor, "refund", checked.lamports, &message.0)
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
                    "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.2.0\",\"event\":\"derived_key\",\"data\":[{{\"trust_id\":{trust_id},\"kind\":\"{kind}\",\"key\":\"{key}\"}}]}}"
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
        &mut self,
        trust_id: u64,
        action: String,
        contributor: String,
        lamports: u64,
        message_hash_hex: String,
        #[callback_result] result: Result<Value, PromiseError>,
    ) -> Option<Value> {
        match result {
            Ok(signature) => {
                env::log_str(&format!(
                    "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.2.0\",\"event\":\"mpc_signature\",\"data\":[{{\"trust_id\":{trust_id},\"action\":\"{action}\",\"message_hash\":\"{message_hash_hex}\"}}]}}"
                ));
                Some(signature)
            }
            Err(_) => {
                self.rollback_sign(trust_id, &action, &contributor, lamports);
                env::log_str("MPC signature request failed");
                None
            }
        }
    }
}

impl Contract {
    fn ensure_contribution(&mut self, trust_id: u64, contributor: &str) -> Contribution {
        let key = ledger_key(trust_id, contributor);
        if let Some(existing) = self.ledger.get(&key) {
            return existing.clone();
        }
        let row = Contribution {
            trust_id,
            contributor: contributor.to_string(),
            deposit_path: contributor_path(trust_id, contributor),
            deposit_address: None,
            reported_lamports: 0,
            credited_lamports: 0,
            overflow_lamports: 0,
            swept_lamports: 0,
            refunded_lamports: 0,
            last_solana_sig: String::new(),
        };
        self.ledger.insert(key, row.clone());
        row
    }

    fn apply_timeouts(&mut self, trust_id: u64) {
        let now = env::block_timestamp();
        let Some(trust) = self.trusts.get_mut(&trust_id) else {
            return;
        };
        match trust.state {
            TrustState::Raising => {
                if now >= trust.deadline_ns && trust.credited_total < trust.target_lamports {
                    trust.state = TrustState::Refunding;
                }
            }
            TrustState::Filled => {
                if let Some(filled_at) = trust.filled_at_ns {
                    if now >= filled_at.saturating_add(trust.execution_timeout_ns) {
                        trust.state = TrustState::Refunding;
                    }
                }
            }
            TrustState::Executing => {
                if let Some(started) = trust.execution_started_ns {
                    if now >= started.saturating_add(trust.execution_timeout_ns) {
                        trust.state = TrustState::Refunding;
                    }
                }
            }
            TrustState::Launched | TrustState::Refunding => {}
        }
    }

    fn raise_dest(&self, trust_id: u64) -> [u8; 32] {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        let addr = trust
            .raise_address
            .as_ref()
            .expect("raise address not derived; call request_derived_key first");
        bs58::decode_pubkey(addr).unwrap_or_else(|e| env::panic_str(e))
    }

    fn optional_deposit_source(&self, trust_id: u64, contributor: &str) -> Option<[u8; 32]> {
        self.ledger
            .get(&ledger_key(trust_id, contributor))
            .and_then(|row| row.deposit_address.as_ref())
            .map(|addr| bs58::decode_pubkey(addr).unwrap_or_else(|e| env::panic_str(e)))
    }

    fn check_transfer(
        &self,
        message: &[u8],
        dest: [u8; 32],
        max_lamports: u64,
        expected_source: Option<[u8; 32]>,
        allow_relayer_fee_payer: bool,
    ) -> shapes::ValidatedTransfer {
        let msg = parse_legacy_message(message).expect("invalid Solana message");
        validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: dest,
                max_lamports,
                expected_source,
                allow_relayer_fee_payer,
            },
        )
        .unwrap_or_else(|e| env::panic_str(e.as_str()))
    }

    fn book_sweep(&mut self, trust_id: u64, contributor: &str, lamports: u64) {
        let key = ledger_key(trust_id, contributor);
        let row = self.ledger.get_mut(&key).expect("unknown contributor");
        row.swept_lamports = row.swept_lamports.saturating_add(lamports);
        if let Some(trust) = self.trusts.get_mut(&trust_id) {
            if trust.state == TrustState::Filled {
                trust.state = TrustState::Executing;
                trust.execution_started_ns = Some(env::block_timestamp());
            }
        }
    }

    fn book_refund(&mut self, trust_id: u64, contributor: &str, lamports: u64) {
        let key = ledger_key(trust_id, contributor);
        let row = self.ledger.get_mut(&key).expect("unknown contributor");
        row.refunded_lamports = row.refunded_lamports.saturating_add(lamports);
    }

    fn rollback_sign(&mut self, trust_id: u64, action: &str, contributor: &str, lamports: u64) {
        let key = ledger_key(trust_id, contributor);
        if let Some(row) = self.ledger.get_mut(&key) {
            if action == "sweep" {
                row.swept_lamports = row.swept_lamports.saturating_sub(lamports);
            } else if action == "refund" {
                row.refunded_lamports = row.refunded_lamports.saturating_sub(lamports);
            }
        }
    }

    fn sign_vault_tx(
        &self,
        trust_id: u64,
        contributor: &str,
        action: &str,
        lamports: u64,
        message: &[u8],
    ) -> Promise {
        let payload_hex = hex::encode(message);
        let message_hash_hex = hex::encode(env::sha256(message));
        let path = contributor_path(trust_id, contributor);
        sign_eddsa_promise(self.mpc_contract.clone(), path, payload_hex).then(
            Self::ext(env::current_account_id())
                .with_static_gas(MPC_CALLBACK_GAS)
                .on_signature_received(
                    trust_id,
                    action.to_string(),
                    contributor.to_string(),
                    lamports,
                    message_hash_hex,
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solana::message::{
        serialize_legacy_message, CompiledInstruction, LegacyMessage, MessageHeader,
    };
    use crate::shapes::SYSTEM_PROGRAM;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    const DEMO_PUBKEY: &str = "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj";
    const SIG_A: &str = "32cEnng3NKyCv2VLQ47M2uXnVDp1LZH7B7rtWZtzHDP2QHz6yrEbFJw7kyWerMs1CYZ2vMoa3cgrcKmKkGNgbd9G";
    const SIG_B: &str = "2wBCwpDikerJ7WiYNu4pRGkyP2QTD5EvHg6mFgnvyw2Q291M72Lpj542ZXnAcxewghaGo14cEg6KHiRZKjtVTMza";
    const SIG_C: &str = "2UwjXHH8ochhUvGhnDx7pSYDwJWUBdsLkav6yEShNXjqWNqMafUTqvU8fu23QEUKWErME8bqY42cvQoU9XEPWr8A";

    fn context(predecessor: AccountId, deposit: NearToken) -> VMContextBuilder {
        let mut b = VMContextBuilder::new();
        b.current_account_id(accounts(0))
            .predecessor_account_id(predecessor)
            .block_timestamp(1_000)
            .attached_deposit(deposit);
        b
    }

    fn context_at(predecessor: AccountId, deposit: NearToken, ts: u64) -> VMContextBuilder {
        let mut b = context(predecessor, deposit);
        b.block_timestamp(ts);
        b
    }

    fn demo_config(target: u64, deadline_ns: u64) -> CreateTrustConfig {
        CreateTrustConfig {
            target_lamports: target,
            deadline_ns,
            dbc_config: "Config1111111111111111111111111111111111111".into(),
            name: "Demo".into(),
            symbol: "DEMO".into(),
            uri: "https://example.com/m.json".into(),
            cap_bps: Some(100),
            fill_fee_bps: Some(150),
            execution_timeout_ns: Some(1_000),
            watcher: None,
        }
    }

    /// 100% cap so a single test wallet can fill the raise.
    fn fill_config(target: u64, deadline_ns: u64) -> CreateTrustConfig {
        let mut cfg = demo_config(target, deadline_ns);
        cfg.cap_bps = Some(10_000);
        cfg
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

    fn relayer_paid_bytes(
        relayer: [u8; 32],
        from: [u8; 32],
        to: [u8; 32],
        lamports: u64,
    ) -> Vec<u8> {
        let mut data = vec![2, 0, 0, 0];
        data.extend_from_slice(&lamports.to_le_bytes());
        serialize_legacy_message(&LegacyMessage {
            header: MessageHeader {
                num_required_signatures: 2,
                num_readonly_signed: 0,
                num_readonly_unsigned: 1,
            },
            account_keys: vec![relayer, from, to, SYSTEM_PROGRAM],
            recent_blockhash: [9u8; 32],
            instructions: vec![CompiledInstruction {
                program_id_index: 3,
                accounts: vec![1, 2],
                data,
            }],
        })
    }

    fn open_trust(c: &mut Contract, target: u64, deadline_ns: u64) -> u64 {
        c.create_trust(demo_config(target, deadline_ns))
    }

    fn open_fillable(c: &mut Contract, target: u64, deadline_ns: u64) -> u64 {
        c.create_trust(fill_config(target, deadline_ns))
    }

    #[test]
    fn create_and_view_trust() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 2_000_000_000, 9_000);
        let trust = c.get_trust(id).unwrap();
        assert_eq!(trust.state, TrustState::Raising);
        assert_eq!(trust.raise_path, "trust/1/raise");
        assert_eq!(trust.fee_path, "trust/1/fees");
        assert_eq!(trust.cap_bps, 100);
        assert_eq!(trust.fill_fee_bps, 150);
        assert_eq!(trust.params_hash.len(), 64);
        assert_eq!(c.fill_fee(id), 0);
        assert_eq!(c.mpc_contract().as_str(), DEFAULT_MPC);
    }

    #[test]
    fn deposit_address_returns_path_and_mpc_inputs() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        let view = c.deposit_address(id, DEMO_PUBKEY.into());
        assert_eq!(view.path, format!("trust/1/in/{DEMO_PUBKEY}"));
        assert_eq!(view.predecessor, accounts(0));
        assert_eq!(view.mpc_contract.as_str(), DEFAULT_MPC);
        assert_eq!(view.domain_id, 1);
    }

    #[test]
    fn register_persists_contributor() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        let path = c.register(id, DEMO_PUBKEY.into());
        assert_eq!(path, format!("trust/1/in/{DEMO_PUBKEY}"));
        let row = c.get_contribution(id, DEMO_PUBKEY.into()).unwrap();
        assert_eq!(row.deposit_path, path);
        assert_eq!(row.credited_lamports, 0);
    }

    #[test]
    fn report_deposit_caps_and_overflows() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        let row = c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        assert_eq!(row.credited_lamports, 10);
        assert_eq!(row.overflow_lamports, 40);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 40);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Raising);
        assert_eq!(c.get_trust(id).unwrap().credited_total, 10);
    }

    #[test]
    fn report_deposit_fills_when_target_met() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Filled);
        assert_eq!(c.sweepable(id, DEMO_PUBKEY.into()), 10);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 0);
    }

    #[test]
    fn missed_deadline_moves_to_refunding() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 2_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 5, SIG_A.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 5);
    }

    #[test]
    fn execution_timeout_from_filled() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 50_000);
        testing_env!(context_at(accounts(2), ONE_YOCTO, 1_000).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Filled);
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 2_500).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
    }

    #[test]
    fn watcher_is_enforced_when_set() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let mut cfg = demo_config(1_000, 9_000);
        cfg.watcher = Some(accounts(3));
        let id = c.create_trust(cfg);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c.report_deposit(id, DEMO_PUBKEY.into(), 1, SIG_A.into());
        }));
        assert!(panicked.is_err());
        testing_env!(context(accounts(3), ONE_YOCTO).build());
        let row = c.report_deposit(id, DEMO_PUBKEY.into(), 1, SIG_B.into());
        assert_eq!(row.credited_lamports, 1);
    }

    #[test]
    fn duplicate_sig_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 1, SIG_A.into());
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c.report_deposit(id, DEMO_PUBKEY.into(), 1, SIG_A.into());
        }));
        assert!(panicked.is_err());
    }

    #[test]
    #[should_panic(expected = "transfer destination is not the fixed owner")]
    fn request_refund_rejects_wrong_destination() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        let owner = bs58::decode_pubkey(DEMO_PUBKEY).unwrap();
        let attacker = [9u8; 32];
        let bytes = transfer_bytes([1u8; 32], attacker, 10);
        let _ = owner;
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer destination is not the fixed owner")]
    fn request_sweep_rejects_attacker_destination() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        let _ = c.on_derived_public_key(id, "raise".into(), Ok(format!("ed25519:{DEMO_PUBKEY}")));
        let bytes = relayer_paid_bytes([8u8; 32], [1u8; 32], [9u8; 32], 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    fn request_sweep_accepts_relayer_paid_raise_transfer() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        let _ = c.on_derived_public_key(id, "raise".into(), Ok(format!("ed25519:{DEMO_PUBKEY}")));
        let raise = bs58::decode_pubkey(DEMO_PUBKEY).unwrap();
        let bytes = relayer_paid_bytes([8u8; 32], [1u8; 32], raise, 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Executing);
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            10
        );
        assert_eq!(c.sweepable(id, DEMO_PUBKEY.into()), 0);
    }

    #[test]
    fn request_refund_overflow_while_raising() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        let dest = bs58::decode_pubkey(DEMO_PUBKEY).unwrap();
        let bytes = relayer_paid_bytes([8u8; 32], [1u8; 32], dest, 40);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .refunded_lamports,
            40
        );
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 0);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Raising);
    }

    #[test]
    #[should_panic(expected = "nothing to refund")]
    fn request_refund_rejects_credited_while_raising() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        let dest = bs58::decode_pubkey(DEMO_PUBKEY).unwrap();
        let bytes = relayer_paid_bytes([8u8; 32], [1u8; 32], dest, 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    fn late_deposit_after_fill_is_all_overflow() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        c.report_deposit(id, DEMO_PUBKEY.into(), 7, SIG_B.into());
        let row = c.get_contribution(id, DEMO_PUBKEY.into()).unwrap();
        assert_eq!(row.credited_lamports, 10);
        assert_eq!(row.overflow_lamports, 7);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Filled);
    }

    #[test]
    fn rollback_restores_swept_on_failed_sign() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_C.into());
        let _ = c.on_derived_public_key(id, "raise".into(), Ok(format!("ed25519:{DEMO_PUBKEY}")));
        let raise = bs58::decode_pubkey(DEMO_PUBKEY).unwrap();
        let bytes = relayer_paid_bytes([8u8; 32], [1u8; 32], raise, 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
        let _ = c.on_signature_received(
            id,
            "sweep".into(),
            DEMO_PUBKEY.into(),
            10,
            "ab".into(),
            Err(PromiseError::Failed),
        );
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            0
        );
    }
}
