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
    raise_refundable_lamports, refundable_lamports, split_deposit, sweepable_lamports,
    DEFAULT_CAP_BPS, DEFAULT_FILL_FEE_BPS, EXECUTION_TIMEOUT_NS, PENDING_EXPIRY_NS,
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
const STORAGE_PREFIX_PENDING: &[u8] = b"p";
const STORAGE_PREFIX_LOCKS: &[u8] = b"k";

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
    pub watcher: AccountId,
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
    pub watcher: AccountId,
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
    pub watcher: AccountId,
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
    pub raise_refunded_lamports: u64,
    pub last_solana_sig: String,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingTransfer {
    pub trust_id: u64,
    pub contributor: String,
    pub kind: String,
    pub lamports: u64,
    pub message_hash: String,
    pub expires_at_ns: u64,
    pub settled: bool,
    /// Explicit `release_pending` after expiry. The row stays so a late
    /// watcher can still `confirm_transfer` the original message hash.
    pub released: bool,
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
    pending: LookupMap<String, PendingTransfer>,
    locks: LookupMap<String, String>,
}

fn ledger_key(trust_id: u64, contributor: &str) -> String {
    format!("{trust_id}:{contributor}")
}

fn lock_key(trust_id: u64, contributor: &str, kind: &str) -> String {
    format!("{trust_id}:{contributor}:{kind}")
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
            pending: LookupMap::new(STORAGE_PREFIX_PENDING),
            locks: LookupMap::new(STORAGE_PREFIX_LOCKS),
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
        let _ = self
            .kick_derive(id, "raise", None)
            .and(self.kick_derive(id, "fees", None));

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
        let row = self.ensure_contribution(trust_id, &solana_pubkey);
        if row.deposit_address.is_none() {
            let _ = self.kick_derive(trust_id, "in", Some(solana_pubkey.clone()));
        }
        let path = row.deposit_path;
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
        require!(
            env::predecessor_account_id() == watcher,
            "only the trusted watcher can report deposits"
        );
        require!(
            state != TrustState::Launched,
            "cannot report deposits after launch"
        );

        let mut row = self.ensure_contribution(trust_id, &contributor);
        if row.deposit_address.is_none() {
            let _ = self.kick_derive(trust_id, "in", Some(contributor.clone()));
        }
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
            )
            .saturating_sub(self.pending_amount(trust_id, &contributor, "refund")),
            None => 0,
        }
    }

    pub fn raise_refundable(&self, trust_id: u64, contributor: String) -> u64 {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        match self.ledger.get(&ledger_key(trust_id, &contributor)) {
            Some(row) => raise_refundable_lamports(
                &trust.state,
                row.swept_lamports,
                row.raise_refunded_lamports,
            )
            .saturating_sub(self.pending_amount(trust_id, &contributor, "raise_refund")),
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
            Some(row) => sweepable_lamports(&trust.state, row.credited_lamports, row.swept_lamports)
                .saturating_sub(self.pending_amount(trust_id, &contributor, "sweep")),
            None => 0,
        }
    }

    pub fn get_pending(&self, message_hash: String) -> Option<PendingTransfer> {
        self.pending.get(&message_hash).cloned()
    }

    /// Ask the MPC signer for the derived Ed25519 key of a Trust path.
    /// `kind` is `raise`, `fees`, or `in` (then `contributor` is required).
    pub fn request_derived_key(
        &self,
        trust_id: u64,
        kind: String,
        contributor: Option<String>,
    ) -> Promise {
        require!(self.trusts.contains_key(&trust_id), "unknown trust");
        require!(
            kind == "raise" || kind == "fees" || kind == "in",
            "kind must be raise, fees, or in"
        );
        if kind == "in" {
            require!(contributor.is_some(), "contributor required for kind=in");
        }
        self.kick_derive(trust_id, &kind, contributor)
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
        let source = self.require_deposit_source(trust_id, &contributor);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        self.lock_pending(trust_id, &contributor, "sweep", checked.lamports, &message.0);
        if let Some(trust) = self.trusts.get_mut(&trust_id) {
            if trust.state == TrustState::Filled {
                trust.state = TrustState::Executing;
                trust.execution_started_ns = Some(env::block_timestamp());
            }
        }
        self.sign_vault_tx(
            contributor_path(trust_id, &contributor),
            trust_id,
            &contributor,
            "sweep",
            checked.lamports,
            &message.0,
        )
    }

    /// Overflow is refundable in Raising, Filled and Executing. Refunding:
    /// everything not already swept from the deposit vault. Destination is
    /// only the contributor's own pubkey.
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
        let source = self.require_deposit_source(trust_id, &contributor);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        self.lock_pending(trust_id, &contributor, "refund", checked.lamports, &message.0);
        self.sign_vault_tx(
            contributor_path(trust_id, &contributor),
            trust_id,
            &contributor,
            "refund",
            checked.lamports,
            &message.0,
        )
    }

    /// Refunding only: move swept SOL from the raise vault back to the
    /// contributor, pro-rata by the amount already swept (credited share).
    #[payable]
    pub fn request_raise_refund(
        &mut self,
        trust_id: u64,
        contributor: String,
        message: Base64VecU8,
    ) -> Promise {
        require!(env::attached_deposit() >= ONE_YOCTO, "attach 1 yoctoNEAR");
        self.apply_timeouts(trust_id);
        let dest = require_pubkey(&contributor);
        let max = self.raise_refundable(trust_id, contributor.clone());
        require!(max > 0, "nothing to refund from raise vault");
        let source = self.raise_dest(trust_id);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        self.lock_pending(
            trust_id,
            &contributor,
            "raise_refund",
            checked.lamports,
            &message.0,
        );
        let path = self
            .trusts
            .get(&trust_id)
            .expect("unknown trust")
            .raise_path
            .clone();
        self.sign_vault_tx(
            path,
            trust_id,
            &contributor,
            "raise_refund",
            checked.lamports,
            &message.0,
        )
    }

    /// Watcher reports that the signed Solana tx landed. Settles the pending
    /// entry keyed by `sha256(message)` hex.
    #[payable]
    pub fn confirm_transfer(&mut self, message_hash: String, solana_sig: String) {
        require!(env::attached_deposit() >= ONE_YOCTO, "attach 1 yoctoNEAR");
        require!(
            solana_sig.len() >= 64 && solana_sig.len() <= 128,
            "solana_sig looks invalid"
        );
        let pending = self
            .pending
            .get(&message_hash)
            .cloned()
            .expect("unknown pending");
        require!(!pending.settled, "pending already settled");
        let watcher = self
            .trusts
            .get(&pending.trust_id)
            .expect("unknown trust")
            .watcher
            .clone();
        require!(
            env::predecessor_account_id() == watcher,
            "only the trusted watcher can confirm transfers"
        );
        self.settle_pending(&pending, &solana_sig);
    }

    /// Anyone may release an expired, unsettled pending so the balance can
    /// be signed again (~90s, covering ~150 Solana slots). The row stays
    /// so `confirm_transfer` can still settle the original hash if it landed.
    pub fn release_pending(&mut self, message_hash: String) {
        let mut pending = self
            .pending
            .get(&message_hash)
            .cloned()
            .expect("unknown pending");
        require!(!pending.settled, "pending already settled");
        require!(!pending.released, "pending already released");
        require!(
            env::block_timestamp() >= pending.expires_at_ns,
            "pending has not expired"
        );
        pending.released = true;
        self.pending.insert(message_hash, pending.clone());
        self.clear_lock(&pending);
    }

    pub fn domain_id(&self) -> u64 {
        DOMAIN_ED25519
    }

    #[private]
    pub fn on_derived_public_key(
        &mut self,
        trust_id: u64,
        kind: String,
        contributor: Option<String>,
        #[callback_result] result: Result<String, PromiseError>,
    ) -> Option<String> {
        match result {
            Ok(key) => {
                let addr = mpc::solana_address_from_near_key(&key);
                if kind == "raise" {
                    if let Some(trust) = self.trusts.get_mut(&trust_id) {
                        trust.raise_address = Some(addr);
                    }
                } else if kind == "fees" {
                    if let Some(trust) = self.trusts.get_mut(&trust_id) {
                        trust.fee_address = Some(addr);
                    }
                } else if kind == "in" {
                    if let Some(pk) = contributor.as_ref() {
                        let key_id = ledger_key(trust_id, pk);
                        if let Some(row) = self.ledger.get_mut(&key_id) {
                            row.deposit_address = Some(addr);
                        }
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
                if let Some(pending) = self.pending.get(&message_hash_hex).cloned() {
                    self.clear_pending(&pending);
                }
                let _ = (action, contributor, lamports);
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
            raise_refunded_lamports: 0,
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

    fn require_deposit_source(&self, trust_id: u64, contributor: &str) -> [u8; 32] {
        let row = self
            .ledger
            .get(&ledger_key(trust_id, contributor))
            .expect("unknown contributor");
        let addr = row
            .deposit_address
            .as_ref()
            .expect("deposit address not derived");
        bs58::decode_pubkey(addr).unwrap_or_else(|e| env::panic_str(e))
    }

    fn kick_derive(&self, trust_id: u64, kind: &str, contributor: Option<String>) -> Promise {
        let path = if kind == "in" {
            contributor_path(
                trust_id,
                contributor.as_deref().expect("contributor required for kind=in"),
            )
        } else {
            derive_path(trust_id, kind)
        };
        derived_public_key_promise(self.mpc_contract.clone(), path, env::current_account_id()).then(
            Self::ext(env::current_account_id())
                .with_static_gas(Gas::from_tgas(10))
                .on_derived_public_key(trust_id, kind.to_string(), contributor),
        )
    }

    fn pending_amount(&self, trust_id: u64, contributor: &str, kind: &str) -> u64 {
        let Some(hash) = self.locks.get(&lock_key(trust_id, contributor, kind)) else {
            return 0;
        };
        match self.pending.get(hash) {
            Some(p) if !p.settled && !p.released => p.lamports,
            _ => 0,
        }
    }

    fn lock_pending(
        &mut self,
        trust_id: u64,
        contributor: &str,
        kind: &str,
        lamports: u64,
        message: &[u8],
    ) {
        require!(
            self.pending_amount(trust_id, contributor, kind) == 0,
            "transfer already pending"
        );
        let message_hash = hex::encode(env::sha256(message));
        require!(
            self.pending
                .get(&message_hash)
                .map(|p| !p.settled)
                .unwrap_or(true),
            "message hash already pending"
        );
        let pending = PendingTransfer {
            trust_id,
            contributor: contributor.to_string(),
            kind: kind.to_string(),
            lamports,
            message_hash: message_hash.clone(),
            expires_at_ns: env::block_timestamp().saturating_add(PENDING_EXPIRY_NS),
            settled: false,
            released: false,
        };
        self.pending.insert(message_hash.clone(), pending);
        self.locks
            .insert(lock_key(trust_id, contributor, kind), message_hash);
    }

    fn settle_pending(&mut self, pending: &PendingTransfer, solana_sig: &str) {
        let key = ledger_key(pending.trust_id, &pending.contributor);
        let row = self.ledger.get_mut(&key).expect("unknown contributor");
        match pending.kind.as_str() {
            "sweep" => row.swept_lamports = row.swept_lamports.saturating_add(pending.lamports),
            "refund" => row.refunded_lamports = row.refunded_lamports.saturating_add(pending.lamports),
            "raise_refund" => {
                row.raise_refunded_lamports =
                    row.raise_refunded_lamports.saturating_add(pending.lamports)
            }
            _ => env::panic_str("unknown pending kind"),
        }
        row.last_solana_sig = solana_sig.to_string();
        let mut settled = pending.clone();
        settled.settled = true;
        self.pending
            .insert(pending.message_hash.clone(), settled);
        self.clear_lock(pending);
    }

    fn clear_lock(&mut self, pending: &PendingTransfer) {
        let lk = lock_key(pending.trust_id, &pending.contributor, &pending.kind);
        if self
            .locks
            .get(&lk)
            .map(|h| h == &pending.message_hash)
            .unwrap_or(false)
        {
            self.locks.remove(&lk);
        }
    }

    fn clear_pending(&mut self, pending: &PendingTransfer) {
        self.pending.remove(&pending.message_hash);
        self.clear_lock(pending);
    }

    fn check_transfer(
        &self,
        message: &[u8],
        dest: [u8; 32],
        max_lamports: u64,
        expected_source: [u8; 32],
        allow_relayer_fee_payer: bool,
    ) -> shapes::ValidatedTransfer {
        let msg = parse_legacy_message(message).expect("invalid Solana message");
        validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: dest,
                max_lamports,
                expected_source: Some(expected_source),
                allow_relayer_fee_payer,
            },
        )
        .unwrap_or_else(|e| env::panic_str(e.as_str()))
    }

    fn sign_vault_tx(
        &self,
        path: String,
        trust_id: u64,
        contributor: &str,
        action: &str,
        lamports: u64,
        message: &[u8],
    ) -> Promise {
        let payload_hex = hex::encode(message);
        let message_hash_hex = hex::encode(env::sha256(message));
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
    const DEPOSIT_ADDR: &str = "F1aZA8xz3ueSpV5CQFB8PGz8y5QXZZtFdZJRTcZQ8DR6";
    const RAISE_ADDR: &str = "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj";
    const OTHER_VAULT: &str = "7bjnioap6J85RWuqPn2CEFRJSSMGvm6CGDR5SsxvxfLa";
    const SIG_A: &str = "32cEnng3NKyCv2VLQ47M2uXnVDp1LZH7B7rtWZtzHDP2QHz6yrEbFJw7kyWerMs1CYZ2vMoa3cgrcKmKkGNgbd9G";
    const SIG_B: &str = "2wBCwpDikerJ7WiYNu4pRGkyP2QTD5EvHg6mFgnvyw2Q291M72Lpj542ZXnAcxewghaGo14cEg6KHiRZKjtVTMza";
    const SIG_C: &str = "2UwjXHH8ochhUvGhnDx7pSYDwJWUBdsLkav6yEShNXjqWNqMafUTqvU8fu23QEUKWErME8bqY42cvQoU9XEPWr8A";
    const SIG_D: &str = "452rHV4vsvxXjAYKXceyeQYmiavoLfAtBbj14kWi9Xdr6S6NCWbRtN9a6WsCi5NLprEiBSeBXdNLhVQKezo6pr9e";

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
            watcher: accounts(2),
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

    fn seed_raise(c: &mut Contract, id: u64) {
        let _ = c.on_derived_public_key(
            id,
            "raise".into(),
            None,
            Ok(format!("ed25519:{RAISE_ADDR}")),
        );
        let _ = c.on_derived_public_key(
            id,
            "fees".into(),
            None,
            Ok(format!("ed25519:{OTHER_VAULT}")),
        );
    }

    fn seed_deposit(c: &mut Contract, id: u64) {
        let _ = c.on_derived_public_key(
            id,
            "in".into(),
            Some(DEMO_PUBKEY.into()),
            Ok(format!("ed25519:{DEPOSIT_ADDR}")),
        );
    }

    fn message_hash(bytes: &[u8]) -> String {
        hex::encode(near_sdk::env::sha256(bytes))
    }

    fn deposit_bytes() -> [u8; 32] {
        bs58::decode_pubkey(DEPOSIT_ADDR).unwrap()
    }

    fn raise_bytes() -> [u8; 32] {
        bs58::decode_pubkey(RAISE_ADDR).unwrap()
    }

    fn owner_bytes() -> [u8; 32] {
        bs58::decode_pubkey(DEMO_PUBKEY).unwrap()
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
        seed_deposit(&mut c, id);
        seed_raise(&mut c, id);
        let row = c.get_contribution(id, DEMO_PUBKEY.into()).unwrap();
        assert_eq!(row.deposit_path, path);
        assert_eq!(row.deposit_address.as_deref(), Some(DEPOSIT_ADDR));
        assert_eq!(row.credited_lamports, 0);
        let trust = c.get_trust(id).unwrap();
        assert_eq!(trust.raise_address.as_deref(), Some(RAISE_ADDR));
        assert_eq!(trust.fee_address.as_deref(), Some(OTHER_VAULT));
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
    fn watcher_is_required() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let mut cfg = demo_config(1_000, 9_000);
        cfg.watcher = accounts(3);
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
        seed_deposit(&mut c, id);
        let attacker = [9u8; 32];
        let bytes = transfer_bytes(deposit_bytes(), attacker, 10);
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
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), [9u8; 32], 10);
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
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Executing);
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            0
        );
        assert_eq!(c.sweepable(id, DEMO_PUBKEY.into()), 0);
        let hash = message_hash(&bytes);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.confirm_transfer(hash, SIG_D.into());
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            10
        );
    }

    #[test]
    fn request_refund_overflow_while_raising() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .refunded_lamports,
            0
        );
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.confirm_transfer(message_hash(&bytes), SIG_D.into());
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
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 10);
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
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 7);
    }

    #[test]
    fn rollback_restores_swept_on_failed_sign() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_C.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        assert!(c.get_pending(hash.clone()).is_some());
        let _ = c.on_signature_received(
            id,
            "sweep".into(),
            DEMO_PUBKEY.into(),
            10,
            hash.clone(),
            Err(PromiseError::Failed),
        );
        assert!(c.get_pending(hash).is_none());
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            0
        );
        assert_eq!(c.sweepable(id, DEMO_PUBKEY.into()), 10);
    }

    #[test]
    #[should_panic(expected = "deposit address not derived")]
    fn request_refund_rejects_when_deposit_address_missing() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_refund_rejects_fee_payer_as_source() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let fee_payer = [8u8; 32];
        let bytes = relayer_paid_bytes(fee_payer, fee_payer, owner_bytes(), 40);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_refund_rejects_other_contributor_vault() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let other = bs58::decode_pubkey(OTHER_VAULT).unwrap();
        let bytes = relayer_paid_bytes([8u8; 32], other, owner_bytes(), 40);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer already pending")]
    fn double_sign_is_blocked_while_pending() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 6);
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), raise_bytes(), 4);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(first));
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(second));
    }

    #[test]
    fn expired_pending_can_be_released() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        testing_env!(context_at(
            accounts(1),
            NearToken::from_yoctonear(0),
            1_000 + PENDING_EXPIRY_NS + 1
        )
        .build());
        c.release_pending(hash.clone());
        let released = c.get_pending(hash).expect("released hash stays stored");
        assert!(released.released);
        assert!(!released.settled);
        assert_eq!(c.sweepable(id, DEMO_PUBKEY.into()), 10);
    }

    #[test]
    #[should_panic(expected = "pending has not expired")]
    fn release_before_expiry_fails() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        c.release_pending(message_hash(&bytes));
    }

    #[test]
    fn request_raise_refund_from_raise_vault() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 2_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let sweep = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(sweep.clone()));
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.confirm_transfer(message_hash(&sweep), SIG_B.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        assert_eq!(c.raise_refundable(id, DEMO_PUBKEY.into()), 10);
        let refund = relayer_paid_bytes([8u8; 32], raise_bytes(), owner_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_raise_refund(id, DEMO_PUBKEY.into(), Base64VecU8(refund.clone()));
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.confirm_transfer(message_hash(&refund), SIG_C.into());
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .raise_refunded_lamports,
            10
        );
        assert_eq!(c.raise_refundable(id, DEMO_PUBKEY.into()), 0);
    }

    #[test]
    fn confirm_transfer_works_after_expiry_without_release() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        testing_env!(context_at(
            accounts(2),
            ONE_YOCTO,
            1_000 + PENDING_EXPIRY_NS + 1
        )
        .build());
        c.confirm_transfer(hash.clone(), SIG_D.into());
        let pending = c.get_pending(hash).unwrap();
        assert!(pending.settled);
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            10
        );
    }

    #[test]
    fn confirm_transfer_works_after_explicit_release() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        testing_env!(context_at(
            accounts(1),
            NearToken::from_yoctonear(0),
            1_000 + PENDING_EXPIRY_NS + 1
        )
        .build());
        c.release_pending(hash.clone());
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.confirm_transfer(hash, SIG_D.into());
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            10
        );
    }

    #[test]
    #[should_panic(expected = "nothing to sweep")]
    fn expired_unreleased_pending_still_blocks_resign() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(first));
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context_at(
            accounts(1),
            ONE_YOCTO,
            1_000 + PENDING_EXPIRY_NS + 1
        )
        .build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(second));
    }

    #[test]
    fn overflow_refund_in_filled() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        c.report_deposit(id, DEMO_PUBKEY.into(), 7, SIG_B.into());
        seed_deposit(&mut c, id);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Filled);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 7);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 7);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.confirm_transfer(message_hash(&bytes), SIG_D.into());
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .refunded_lamports,
            7
        );
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Filled);
    }

    #[test]
    #[should_panic(expected = "only the trusted watcher can confirm transfers")]
    fn confirm_transfer_rejects_non_watcher() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        c.confirm_transfer(message_hash(&bytes), SIG_D.into());
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_raise_refund_rejects_deposit_as_source() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 2_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let sweep = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(sweep.clone()));
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.confirm_transfer(message_hash(&sweep), SIG_B.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        let refund = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_raise_refund(id, DEMO_PUBKEY.into(), Base64VecU8(refund));
    }

    #[test]
    #[should_panic(expected = "deposit address not derived")]
    fn request_sweep_rejects_when_deposit_address_missing() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_sweep_rejects_fee_payer_as_source() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let fee_payer = [8u8; 32];
        let bytes = relayer_paid_bytes(fee_payer, fee_payer, raise_bytes(), 10);
        testing_env!(context(accounts(1), ONE_YOCTO).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }
}
