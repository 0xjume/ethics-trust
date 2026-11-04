//! Keyless Trust — NEAR contract that will only sign allowed Solana vault txs.
//!
//! Milestone 4: chip-in / refund / sweep, DBC Shape C launch, pro-rata
//! Shape D distribution.

use near_sdk::json_types::Base64VecU8;
use near_sdk::serde_json::Value;
use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{
    env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError,
};

mod bs58;
mod math;
mod mpc;
mod pda;
mod quote;
pub mod shapes;
pub mod solana;

use math::{
    apply_rent_dust_closeout, fill_fee_lamports, pro_rata_share, raise_refundable_lamports,
    refundable_lamports, split_deposit, sweepable_lamports, DEFAULT_FILL_FEE_BPS,
    EXECUTION_TIMEOUT_NS, MAX_TOKENS_BOUGHT, PENDING_EXPIRY_NS,
};
use mpc::{
    contributor_path, derive_path, derived_public_key_promise, sign_eddsa_promise, DOMAIN_ED25519,
    MPC_CALLBACK_GAS, ONE_YOCTO,
};
use quote::{append_curve_bytes, quote_first_buy, validate_curve, DbcCurve};
use shapes::dbc::TOKEN_PROGRAM;
use shapes::{
    decode_dbc_program, parse_distribute, resolve_owners, validate_launch,
    validate_system_transfer, LaunchCheck, TransferCheck, MAX_DISTRIBUTE_BATCH, TOKEN_DECIMALS,
};
use solana::parse_legacy_message;

const DEFAULT_MPC: &str = "v1.signer-prod.testnet";
const DEFAULT_DBC: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
const STORAGE_PREFIX_TRUSTS: &[u8] = b"t";
const STORAGE_PREFIX_LEDGER: &[u8] = b"c";
const STORAGE_PREFIX_SIGS: &[u8] = b"s";
const STORAGE_PREFIX_PENDING: &[u8] = b"p";
const STORAGE_PREFIX_LOCKS: &[u8] = b"k";
const STORAGE_PREFIX_CONSUMED: &[u8] = b"h";
const STORAGE_PREFIX_NONCES: &[u8] = b"n";
const STORAGE_PREFIX_RELEASED: &[u8] = b"r";

/// Covers the contribution row plus the later `derived_public_key` callback
/// write (~deposit address). Excess is refunded.
pub const REGISTER_DEPOSIT: NearToken = NearToken::from_millinear(5);
/// Worst-case pending row + lock + nonce + later consumed-hash write.
/// Live sign needed ~0.0043 NEAR; 0.001 failed. 0.01 NEAR covers it.
/// Excess is refunded.
pub const SIGN_DEPOSIT: NearToken = NearToken::from_millinear(10);
/// Caller must attach at least this much gas to `request_launch`: two
/// MPC `sign` calls (30 TGas each) plus the 20 TGas callback.
pub const LAUNCH_ATTACH_TGAS: u64 = 90;
/// One raise-vault MPC `sign` (30 TGas) plus the 20 TGas callback, with
/// headroom for the Shape D checker. Live direct `sign` burnt 8.84 TGas.
pub const DISTRIBUTE_ATTACH_TGAS: u64 = 60;

#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TrustState {
    Raising,
    Filled,
    Executing,
    Launched,
    Refunding,
    /// Every credited contributor has been paid their floor share.
    /// Rounding dust stays in the raise ATA and is not a share.
    Distributed,
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
    pub cap_bps: u16,
    pub fill_fee_bps: Option<u16>,
    pub execution_timeout_ns: Option<u64>,
    pub watcher: AccountId,
    /// Pinned DBC curve. The first-buy quote is computed from this, not from
    /// a watcher balance. Committed in `params_hash`.
    pub curve: DbcCurve,
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
    pub mint_path: String,
    pub mint_address: Option<String>,
    pub pool_address: Option<String>,
    pub fill_fee_paid: u64,
    pub dbc_config: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    /// Curve quoted at `request_launch`. Committed in `params_hash`.
    pub curve: DbcCurve,
    pub params_hash: String,
    pub credited_total: u64,
    pub overflow_total: u64,
    /// First-buy output quoted at `request_launch` and pinned as
    /// `minimum_amount_out`. 0 until launch is requested. Confirm does not
    /// change it. Surplus the swap delivers above this stays in the raise ATA.
    pub tokens_bought: u64,
    /// Sum of floor shares. Dust is `tokens_bought - claimable_total`.
    pub claimable_total: u64,
    pub distributed_total: u64,
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
    pub mint_path: String,
    pub mint_address: Option<String>,
    pub pool_address: Option<String>,
    pub fill_fee_paid: u64,
    pub dbc_config: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub curve: DbcCurve,
    pub params_hash: String,
    pub credited_total: u64,
    pub overflow_total: u64,
    pub tokens_bought: u64,
    pub claimable_total: u64,
    pub distributed_total: u64,
    pub watcher: AccountId,
    pub state: TrustState,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct DistributionStatus {
    pub trust_id: u64,
    pub state: TrustState,
    pub tokens_bought: u64,
    pub claimable_total: u64,
    pub distributed_total: u64,
    /// `tokens_bought - claimable_total`. Stays in the raise ATA.
    pub dust_tokens: u64,
    pub decimals: u8,
    /// Classic SPL Tokenkeg. Shape C never initializes Token-2022.
    pub token_program: String,
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
            mint_path: trust.mint_path.clone(),
            mint_address: trust.mint_address.clone(),
            pool_address: trust.pool_address.clone(),
            fill_fee_paid: trust.fill_fee_paid,
            dbc_config: trust.dbc_config.clone(),
            name: trust.name.clone(),
            symbol: trust.symbol.clone(),
            uri: trust.uri.clone(),
            curve: trust.curve.clone(),
            params_hash: trust.params_hash.clone(),
            credited_total: trust.credited_total,
            overflow_total: trust.overflow_total,
            tokens_bought: trust.tokens_bought,
            claimable_total: trust.claimable_total,
            distributed_total: trust.distributed_total,
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
    /// Raw tokens booked on `confirm_transfer` of a distribute leg.
    /// Not updated at sign time.
    pub distributed_tokens: u64,
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
    /// watcher can still `confirm_transfer` the original message hash
    /// if no later sign took the same lock.
    pub released: bool,
    /// Per-lock generation. A re-sign increments it; older released
    /// rows are then superseded.
    pub nonce: u64,
    pub superseded: bool,
    /// Launch: `pool_base58:fill_fee_in_tx`. Distribute: comma-separated
    /// `owner_b58:amount:nonce` legs (base58 has no colon or comma).
    /// Empty for sweep / refund.
    pub extra: String,
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
    consumed_hashes: LookupMap<String, u64>,
    lock_nonces: LookupMap<String, u64>,
    last_released: LookupMap<String, String>,
}

fn ledger_key(trust_id: u64, contributor: &str) -> String {
    format!("{trust_id}:{contributor}")
}

fn lock_key(trust_id: u64, contributor: &str, kind: &str) -> String {
    format!("{trust_id}:{contributor}:{kind}")
}

fn projected_state(trust: &Trust) -> TrustState {
    let now = env::block_timestamp();
    match &trust.state {
        TrustState::Raising => {
            if now >= trust.deadline_ns && trust.credited_total < trust.target_lamports {
                TrustState::Refunding
            } else {
                TrustState::Raising
            }
        }
        TrustState::Filled => {
            if let Some(filled_at) = trust.filled_at_ns {
                if now >= filled_at.saturating_add(trust.execution_timeout_ns) {
                    return TrustState::Refunding;
                }
            }
            TrustState::Filled
        }
        TrustState::Executing => {
            if let Some(started) = trust.execution_started_ns {
                if now >= started.saturating_add(trust.execution_timeout_ns) {
                    return TrustState::Refunding;
                }
            }
            TrustState::Executing
        }
        other => other.clone(),
    }
}

fn pin_params_hash(
    dbc_config: &str,
    name: &str,
    symbol: &str,
    uri: &str,
    curve: &DbcCurve,
) -> String {
    let mut buf = Vec::with_capacity(dbc_config.len() + name.len() + symbol.len() + uri.len() + 64);
    buf.extend_from_slice(dbc_config.as_bytes());
    buf.push(0);
    buf.extend_from_slice(name.as_bytes());
    buf.push(0);
    buf.extend_from_slice(symbol.as_bytes());
    buf.push(0);
    buf.extend_from_slice(uri.as_bytes());
    append_curve_bytes(&mut buf, curve);
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
            consumed_hashes: LookupMap::new(STORAGE_PREFIX_CONSUMED),
            lock_nonces: LookupMap::new(STORAGE_PREFIX_NONCES),
            last_released: LookupMap::new(STORAGE_PREFIX_RELEASED),
        }
    }

    pub fn mpc_contract(&self) -> AccountId {
        self.mpc_contract.clone()
    }

    pub fn dbc_program(&self) -> String {
        self.dbc_program.clone()
    }

    pub fn get_trust(&self, trust_id: u64) -> Option<TrustView> {
        self.trusts.get(&trust_id).map(|t| self.view_trust(t))
    }

    pub fn list_trusts(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<TrustView> {
        let skip = from_index.unwrap_or(0) as usize;
        let take = limit.unwrap_or(20).min(50) as usize;
        self.trusts
            .values()
            .skip(skip)
            .take(take)
            .map(|t| self.view_trust(t))
            .collect()
    }

    pub fn get_contribution(&self, trust_id: u64, contributor: String) -> Option<Contribution> {
        self.ledger
            .get(&ledger_key(trust_id, &contributor))
            .cloned()
    }

    pub fn derivation_paths(&self, trust_id: u64, solana_pubkey: Option<String>) -> Vec<String> {
        let mut paths = vec![
            derive_path(trust_id, "raise"),
            derive_path(trust_id, "fees"),
            derive_path(trust_id, "mint"),
        ];
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
        let _ = require_pubkey(&config.dbc_config);
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
        let cap = config.cap_bps;
        let fill_fee = config.fill_fee_bps.unwrap_or(DEFAULT_FILL_FEE_BPS);
        let timeout = config.execution_timeout_ns.unwrap_or(EXECUTION_TIMEOUT_NS);
        require!(cap > 0 && cap <= 10_000, "cap_bps out of range");
        require!(fill_fee <= 10_000, "fill_fee_bps out of range");
        require!(timeout > 0, "execution_timeout_ns must be > 0");
        validate_curve(&config.curve).unwrap_or_else(|e| env::panic_str(e));

        let params_hash = pin_params_hash(
            &config.dbc_config,
            &config.name,
            &config.symbol,
            &config.uri,
            &config.curve,
        );
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
            mint_path: derive_path(id, "mint"),
            mint_address: None,
            pool_address: None,
            fill_fee_paid: 0,
            dbc_config: config.dbc_config,
            name: config.name,
            symbol: config.symbol,
            uri: config.uri,
            curve: config.curve,
            params_hash,
            credited_total: 0,
            overflow_total: 0,
            tokens_bought: 0,
            claimable_total: 0,
            distributed_total: 0,
            watcher: config.watcher,
            state: TrustState::Raising,
        };
        self.trusts.insert(id, trust);
        let _ = self
            .kick_derive(id, "raise", None)
            .and(self.kick_derive(id, "fees", None))
            .and(self.kick_derive(id, "mint", None));

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.2.0\",\"event\":\"trust_created\",\"data\":[{{\"trust_id\":{id}}}]}}"
        ));
        id
    }

    /// Anyone may register any Solana pubkey. Refunds can only go to that pubkey.
    /// Attach `register_deposit` (covers the row + derive callback); excess is refunded.
    #[payable]
    pub fn register(&mut self, trust_id: u64, solana_pubkey: String) -> String {
        require!(
            env::attached_deposit() >= REGISTER_DEPOSIT,
            "insufficient storage deposit"
        );
        let before = self.storage_snapshot();
        require!(self.trusts.contains_key(&trust_id), "unknown trust");
        self.apply_timeouts(trust_id);
        let state = self
            .trusts
            .get(&trust_id)
            .expect("unknown trust")
            .state
            .clone();
        require!(
            state != TrustState::Launched
                && state != TrustState::Refunding
                && state != TrustState::Distributed,
            "cannot register after the trust is closed"
        );
        let _ = require_pubkey(&solana_pubkey);
        let row = self.ensure_contribution(trust_id, &solana_pubkey);
        if row.deposit_address.is_none() {
            let _ = self.kick_derive(trust_id, "in", Some(solana_pubkey.clone()));
        }
        let path = row.deposit_path;
        self.charge_storage(before, REGISTER_DEPOSIT);
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
            state != TrustState::Launched && state != TrustState::Distributed,
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
        let state = projected_state(&trust);
        match self.ledger.get(&ledger_key(trust_id, &contributor)) {
            Some(row) => {
                let pending_sweep = if state == TrustState::Refunding {
                    self.pending_amount(trust_id, &contributor, "sweep")
                } else {
                    0
                };
                let remaining = row
                    .credited_lamports
                    .saturating_add(row.overflow_lamports)
                    .saturating_sub(row.swept_lamports)
                    .saturating_sub(row.refunded_lamports);
                let base = refundable_lamports(
                    &state,
                    row.credited_lamports,
                    row.overflow_lamports,
                    row.swept_lamports,
                    row.refunded_lamports,
                );
                apply_rent_dust_closeout(base, remaining)
                    .saturating_sub(self.pending_amount(trust_id, &contributor, "refund"))
                    .saturating_sub(pending_sweep)
            }
            None => 0,
        }
    }

    pub fn raise_refundable(&self, trust_id: u64, contributor: String) -> u64 {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        let state = projected_state(&trust);
        match self.ledger.get(&ledger_key(trust_id, &contributor)) {
            Some(row) => {
                raise_refundable_lamports(&state, row.swept_lamports, row.raise_refunded_lamports)
                    .saturating_sub(self.pending_amount(trust_id, &contributor, "raise_refund"))
            }
            None => 0,
        }
    }

    pub fn fill_fee(&self, trust_id: u64) -> u64 {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        math::fill_fee_lamports(trust.credited_total, trust.fill_fee_bps)
    }

    pub fn swept_total(&self, trust_id: u64) -> u64 {
        self.ledger
            .values()
            .filter(|row| row.trust_id == trust_id)
            .map(|row| row.swept_lamports)
            .sum()
    }

    pub fn launch_attach_tgas(&self) -> u64 {
        LAUNCH_ATTACH_TGAS
    }

    /// Gas the caller must attach to `request_distribute` (one 30 TGas
    /// raise `sign` + 20 TGas callback + checker headroom).
    pub fn distribute_attach_tgas(&self) -> u64 {
        DISTRIBUTE_ATTACH_TGAS
    }

    pub fn max_distribute_batch(&self) -> u64 {
        MAX_DISTRIBUTE_BATCH as u64
    }

    /// Remaining floor share, minus tokens already booked and any live
    /// (unreleased) distribute lock for this contributor.
    pub fn claimable(&self, trust_id: u64, contributor: String) -> u64 {
        self.claimable_of(trust_id, &contributor)
    }

    pub fn distribution_status(&self, trust_id: u64) -> DistributionStatus {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        let state = projected_state(trust);
        DistributionStatus {
            trust_id,
            state,
            tokens_bought: trust.tokens_bought,
            claimable_total: trust.claimable_total,
            distributed_total: trust.distributed_total,
            dust_tokens: trust.tokens_bought.saturating_sub(trust.claimable_total),
            decimals: TOKEN_DECIMALS,
            token_program: crate::bs58::encode(&TOKEN_PROGRAM),
        }
    }

    pub fn sweepable(&self, trust_id: u64, contributor: String) -> u64 {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        let state = projected_state(&trust);
        match self.ledger.get(&ledger_key(trust_id, &contributor)) {
            Some(row) => sweepable_lamports(&state, row.credited_lamports, row.swept_lamports)
                .saturating_sub(self.pending_amount(trust_id, &contributor, "sweep")),
            None => 0,
        }
    }

    pub fn get_pending(&self, message_hash: String) -> Option<PendingTransfer> {
        self.pending.get(&message_hash).cloned()
    }

    pub fn is_consumed(&self, message_hash: String) -> bool {
        self.consumed_hashes.get(&message_hash).is_some()
    }

    pub fn register_deposit(&self) -> NearToken {
        REGISTER_DEPOSIT
    }

    pub fn sign_deposit(&self) -> NearToken {
        SIGN_DEPOSIT
    }

    pub fn pending_expiry_ns(&self) -> u64 {
        PENDING_EXPIRY_NS
    }

    /// Ask the MPC signer for the derived Ed25519 key of a Trust path.
    /// `kind` is `raise`, `fees`, `mint`, or `in` (then `contributor` is required).
    pub fn request_derived_key(
        &self,
        trust_id: u64,
        kind: String,
        contributor: Option<String>,
    ) -> Promise {
        require!(self.trusts.contains_key(&trust_id), "unknown trust");
        require!(
            kind == "raise" || kind == "fees" || kind == "mint" || kind == "in",
            "kind must be raise, fees, mint, or in"
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
    /// Watcher-only so an unprivileged account cannot lock a contributor's
    /// sweep pending for 150 s.
    #[payable]
    pub fn request_sweep(
        &mut self,
        trust_id: u64,
        contributor: String,
        message: Base64VecU8,
    ) -> Promise {
        require!(
            env::attached_deposit() >= SIGN_DEPOSIT,
            "insufficient storage deposit"
        );
        let before = self.storage_snapshot();
        self.apply_timeouts(trust_id);
        self.require_watcher(trust_id);
        let dest = self.raise_dest(trust_id);
        let max = self.sweepable(trust_id, contributor.clone());
        require!(max > 0, "nothing to sweep");
        let source = self.require_deposit_source(trust_id, &contributor);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        self.lock_pending(
            trust_id,
            &contributor,
            "sweep",
            checked.lamports,
            &message.0,
            String::new(),
        );
        self.charge_storage(before, SIGN_DEPOSIT);
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
    /// only the contributor's own pubkey. Amount must be the full
    /// refundable (including rent-dust close-out). Permissionless so a
    /// contributor can still exit if the watcher is offline.
    #[payable]
    pub fn request_refund(
        &mut self,
        trust_id: u64,
        contributor: String,
        message: Base64VecU8,
    ) -> Promise {
        require!(
            env::attached_deposit() >= SIGN_DEPOSIT,
            "insufficient storage deposit"
        );
        let before = self.storage_snapshot();
        self.apply_timeouts(trust_id);
        let dest = require_pubkey(&contributor);
        let max = self.refundable(trust_id, contributor.clone());
        require!(max > 0, "nothing to refund");
        let source = self.require_deposit_source(trust_id, &contributor);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        require!(
            checked.lamports == max,
            "refund must be the full refundable amount"
        );
        self.lock_pending(
            trust_id,
            &contributor,
            "refund",
            checked.lamports,
            &message.0,
            String::new(),
        );
        self.charge_storage(before, SIGN_DEPOSIT);
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
        require!(
            env::attached_deposit() >= SIGN_DEPOSIT,
            "insufficient storage deposit"
        );
        let before = self.storage_snapshot();
        self.apply_timeouts(trust_id);
        let dest = require_pubkey(&contributor);
        let max = self.raise_refundable(trust_id, contributor.clone());
        require!(max > 0, "nothing to refund from raise vault");
        let source = self.raise_dest(trust_id);
        let checked = self.check_transfer(&message.0, dest, max, source, true);
        require!(
            checked.lamports == max,
            "refund must be the full refundable amount"
        );
        self.lock_pending(
            trust_id,
            &contributor,
            "raise_refund",
            checked.lamports,
            &message.0,
            String::new(),
        );
        self.charge_storage(before, SIGN_DEPOSIT);
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

    /// Optional prior Shape: exact 1.5% fill fee from the raise vault to the
    /// fee vault. Alternative is including the same transfer inside the
    /// launch message. Executing only.
    #[payable]
    pub fn request_fill_fee(&mut self, trust_id: u64, message: Base64VecU8) -> Promise {
        require!(
            env::attached_deposit() >= SIGN_DEPOSIT,
            "insufficient storage deposit"
        );
        let before = self.storage_snapshot();
        self.apply_timeouts(trust_id);
        let (state, credited, fill_fee_bps, fill_fee_paid, raise_path) = {
            let trust = self.trusts.get(&trust_id).expect("unknown trust");
            (
                trust.state.clone(),
                trust.credited_total,
                trust.fill_fee_bps,
                trust.fill_fee_paid,
                trust.raise_path.clone(),
            )
        };
        require!(state == TrustState::Executing, "fill fee only in Executing");
        self.require_watcher(trust_id);
        let expected = fill_fee_lamports(credited, fill_fee_bps);
        require!(expected > 0, "nothing to take as fill fee");
        require!(fill_fee_paid < expected, "fill fee already paid");
        let dest = self.fee_dest(trust_id);
        let source = self.raise_dest(trust_id);
        let checked = self.check_transfer(&message.0, dest, expected, source, true);
        require!(checked.lamports == expected, "fill fee must be exact");
        self.lock_pending(
            trust_id,
            "",
            "fill_fee",
            checked.lamports,
            &message.0,
            String::new(),
        );
        self.charge_storage(before, SIGN_DEPOSIT);
        self.sign_vault_tx(
            raise_path,
            trust_id,
            "",
            "fill_fee",
            checked.lamports,
            &message.0,
        )
    }

    /// Shape C: DBC initialize_virtual_pool_with_spl_token + first-buy swap.
    /// Executing only, after every sweep has been confirmed. Two MPC signs
    /// (raise + mint at `trust/<id>/mint`). Attach ≥ `launch_attach_tgas()`
    /// (90 TGas). The quote is local arithmetic on the pinned curve (no extra
    /// promise), so that gas budget is unchanged. Watcher-only — anyone else
    /// locking launch for 150 s is griefing.
    ///
    /// `minimum_amount_out` must equal the contract's exact-in quote of
    /// `credited − fill fee`. That quote is stored as `tokens_bought` here.
    /// Confirm does not take a balance.
    #[payable]
    pub fn request_launch(&mut self, trust_id: u64, message: Base64VecU8) -> Promise {
        require!(
            env::attached_deposit() >= SIGN_DEPOSIT,
            "insufficient storage deposit"
        );
        let before = self.storage_snapshot();
        self.apply_timeouts(trust_id);
        let (
            state,
            credited,
            raise_addr,
            fee_addr,
            mint_addr,
            dbc_config,
            name,
            symbol,
            uri,
            fill_fee_bps,
            fill_fee_paid,
            raise_path,
            mint_path,
            curve,
        ) = {
            let trust = self.trusts.get(&trust_id).expect("unknown trust");
            (
                trust.state.clone(),
                trust.credited_total,
                trust.raise_address.clone(),
                trust.fee_address.clone(),
                trust.mint_address.clone(),
                trust.dbc_config.clone(),
                trust.name.clone(),
                trust.symbol.clone(),
                trust.uri.clone(),
                trust.fill_fee_bps,
                trust.fill_fee_paid,
                trust.raise_path.clone(),
                trust.mint_path.clone(),
                trust.curve.clone(),
            )
        };
        require!(state == TrustState::Executing, "launch only in Executing");
        self.require_watcher(trust_id);
        require!(credited > 0, "nothing to launch");
        require!(self.swept_total(trust_id) == credited, "sweep not settled");
        require!(!self.any_pending_sweep(trust_id), "sweep still pending");
        require!(
            self.pending_amount(trust_id, "", "fill_fee") == 0,
            "fill fee still pending"
        );
        let raise = require_pubkey(
            raise_addr
                .as_ref()
                .expect("raise address not derived; call request_derived_key first"),
        );
        let fee = require_pubkey(
            fee_addr
                .as_ref()
                .expect("fee address not derived; call request_derived_key first"),
        );
        let mint = require_pubkey(
            mint_addr
                .as_ref()
                .expect("mint address not derived; call request_derived_key first"),
        );
        let config = require_pubkey(&dbc_config);
        let dbc_program = decode_dbc_program(&self.dbc_program)
            .unwrap_or_else(|_| env::panic_str("invalid dbc_program"));
        let fee_lamports = fill_fee_lamports(credited, fill_fee_bps);
        let buy = credited.saturating_sub(fee_lamports);
        require!(buy > 0, "nothing to buy");
        let quoted = quote_first_buy(&curve, buy).unwrap_or_else(|e| env::panic_str(e));
        let already_paid = fill_fee_paid >= fee_lamports;
        let msg = parse_legacy_message(&message.0).expect("invalid Solana message");
        let checked = validate_launch(
            &msg,
            &LaunchCheck {
                dbc_program,
                config,
                raise,
                fee,
                mint,
                name,
                symbol,
                uri,
                buy_lamports: buy,
                fill_fee_lamports: fee_lamports,
                fill_fee_already_paid: already_paid,
                min_out: quoted,
            },
        )
        .unwrap_or_else(|e| env::panic_str(e.as_str()));
        {
            let trust = self.trusts.get_mut(&trust_id).expect("unknown trust");
            if trust.tokens_bought != 0 {
                require!(
                    trust.tokens_bought == quoted,
                    "tokens bought does not match the quote"
                );
            }
            trust.tokens_bought = quoted;
        }
        let extra = format!(
            "{}:{}",
            crate::bs58::encode(&checked.pool),
            checked.fill_fee_in_tx
        );
        self.lock_pending(
            trust_id,
            "",
            "launch",
            checked.buy_lamports,
            &message.0,
            extra,
        );
        self.charge_storage(before, SIGN_DEPOSIT);
        self.sign_launch_tx(raise_path, mint_path, trust_id, &message.0)
    }

    /// Shape D. Permissionless claim of one contributor's remaining floor
    /// share, or a batch of up to `max_distribute_batch` shares, from the
    /// raise vault's token ATA. One raise-vault MPC sign. Books nothing
    /// until the watcher `confirm_transfer`s. Each contributor has their
    /// own lock, so a stranger who pays `sign_deposit` (0.01 NEAR, excess
    /// refunded) can freeze only the contributors included in this message
    /// for `pending_expiry_ns` (150 s), not the rest of the raise.
    ///
    /// A batch locks every included contributor in that one call. That is
    /// the grief trade-off of batching: at most 5 claims, for one 0.01
    /// NEAR deposit, for 150 s. Leaving someone out does not freeze them.
    #[payable]
    pub fn request_distribute(&mut self, trust_id: u64, message: Base64VecU8) -> Promise {
        require!(
            env::attached_deposit() >= SIGN_DEPOSIT,
            "insufficient storage deposit"
        );
        require!(
            env::prepaid_gas() >= Gas::from_tgas(DISTRIBUTE_ATTACH_TGAS),
            "attach at least 60 TGas"
        );
        let before = self.storage_snapshot();
        self.apply_timeouts(trust_id);
        let (state, raise_addr, mint_addr, fee_addr, tokens_bought, raise_path) = {
            let trust = self.trusts.get(&trust_id).expect("unknown trust");
            (
                projected_state(trust),
                trust.raise_address.clone(),
                trust.mint_address.clone(),
                trust.fee_address.clone(),
                trust.tokens_bought,
                trust.raise_path.clone(),
            )
        };
        require!(state == TrustState::Launched, "distribute only in Launched");
        require!(tokens_bought > 0, "tokens bought not recorded");
        let raise = require_pubkey(
            raise_addr
                .as_ref()
                .expect("raise address not derived; call request_derived_key first"),
        );
        let mint = require_pubkey(
            mint_addr
                .as_ref()
                .expect("mint address not derived; call request_derived_key first"),
        );
        let fee = require_pubkey(
            fee_addr
                .as_ref()
                .expect("fee address not derived; call request_derived_key first"),
        );
        let msg = parse_legacy_message(&message.0).unwrap_or_else(|e| env::panic_str(e.as_str()));
        let parsed = parse_distribute(&msg, message.0.len(), &raise, &mint, &fee)
            .unwrap_or_else(|e| env::panic_str(e.as_str()));
        let mut expected = Vec::with_capacity(parsed.len());
        for leg in &parsed {
            let owner_pk = if leg.owner_resolved {
                self.require_distribute_owner(trust_id, &crate::bs58::encode(&leg.owner));
                leg.owner
            } else {
                let candidates = self.distribute_candidates(trust_id);
                let owner = candidates.iter().find(|(pk, _)| {
                    shapes::distribute::derive_ata(pk, &mint, &TOKEN_PROGRAM) == leg.dest
                });
                let Some((pk, _)) = owner else {
                    env::panic_str("destination owner is not the registered contributor");
                };
                *pk
            };
            let owner = crate::bs58::encode(&owner_pk);
            let remaining = self.claimable_of(trust_id, &owner);
            require!(remaining > 0, "nothing to claim");
            expected.push((owner_pk, remaining));
        }
        let legs =
            resolve_owners(parsed, &mint, &expected).unwrap_or_else(|e| env::panic_str(e.as_str()));
        let mut locked: Vec<(String, u64)> = Vec::with_capacity(legs.len());
        for leg in &legs {
            let owner = crate::bs58::encode(&leg.owner);
            let remaining = self.claimable_of(trust_id, &owner);
            require!(remaining > 0, "nothing to claim");
            require!(
                leg.amount == remaining,
                "transfer amount is not the remaining share"
            );
            locked.push((owner, leg.amount));
        }
        self.lock_distribute(trust_id, &locked, &message.0);
        self.charge_storage(before, SIGN_DEPOSIT);
        let total = locked.iter().map(|(_, a)| *a).sum();
        self.sign_vault_tx(
            raise_path,
            trust_id,
            &locked[0].0,
            "distribute",
            total,
            &message.0,
        )
    }

    /// Watcher reports that the signed Solana tx landed. Settles the pending
    /// entry keyed by `sha256(message)` hex. Sweep, refund, fill-fee, and
    /// distribute. Launch pendings use `confirm_launch_outcome`.
    #[payable]
    pub fn confirm_transfer(&mut self, message_hash: String, solana_sig: String) {
        self.confirm_inner(message_hash, solana_sig, false);
    }

    /// Launch confirm. The share basis is the quote stored at
    /// `request_launch`. This method does not take `tokens_bought`. Any
    /// tokens the swap delivered above `minimum_amount_out` stay in the
    /// raise ATA and are not claimable.
    #[payable]
    pub fn confirm_launch_outcome(&mut self, message_hash: String, solana_sig: String) {
        self.confirm_inner(message_hash, solana_sig, true);
    }

    fn confirm_inner(&mut self, message_hash: String, solana_sig: String, launch: bool) {
        require!(env::attached_deposit() >= ONE_YOCTO, "attach 1 yoctoNEAR");
        require!(
            solana_sig.len() >= 64 && solana_sig.len() <= 128,
            "solana_sig looks invalid"
        );
        require!(
            self.consumed_hashes.get(&message_hash).is_none(),
            "pending already settled"
        );
        let pending = self
            .pending
            .get(&message_hash)
            .cloned()
            .expect("unknown pending");
        require!(!pending.settled, "pending already settled");
        require!(!pending.superseded, "pending superseded");
        self.apply_timeouts(pending.trust_id);
        require!(
            (pending.kind == "launch") == launch,
            "use confirm_launch_outcome for launch and confirm_transfer otherwise"
        );
        if pending.kind == "distribute" {
            for leg in parse_distribute_extra(&pending.extra) {
                let lk = lock_key(pending.trust_id, &leg.contributor, "distribute");
                let current = self.lock_nonces.get(&lk).copied().unwrap_or(0);
                require!(leg.nonce == current, "pending superseded");
                if pending.released {
                    let remaining = self.claimable_of(pending.trust_id, &leg.contributor);
                    require!(leg.amount <= remaining, "amount exceeds remaining budget");
                }
            }
        } else {
            let lk = lock_key(pending.trust_id, &pending.contributor, &pending.kind);
            let current_nonce = self.lock_nonces.get(&lk).copied().unwrap_or(0);
            require!(pending.nonce == current_nonce, "pending superseded");
            if pending.released {
                let remaining =
                    self.remaining_budget(pending.trust_id, &pending.contributor, &pending.kind);
                require!(
                    pending.lamports <= remaining,
                    "amount exceeds remaining budget"
                );
            }
        }
        require!(
            self.seen_sigs.get(&solana_sig).is_none(),
            "solana_sig already used"
        );
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
    /// be signed again (~150s, past the Solana blockhash lifetime). The row
    /// stays so a late `confirm_transfer` can still book if no later sign
    /// took the same lock. Settled rows are pruned.
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
        self.pending.insert(message_hash.clone(), pending.clone());
        if pending.kind == "distribute" {
            for leg in parse_distribute_extra(&pending.extra) {
                self.last_released.insert(
                    lock_key(pending.trust_id, &leg.contributor, "distribute"),
                    message_hash.clone(),
                );
            }
        } else {
            self.last_released.insert(
                lock_key(pending.trust_id, &pending.contributor, &pending.kind),
                message_hash,
            );
        }
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
                } else if kind == "mint" {
                    if let Some(trust) = self.trusts.get_mut(&trust_id) {
                        trust.mint_address = Some(addr);
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
                    self.revert_lock_nonce(&pending);
                    self.clear_pending(&pending);
                }
                let _ = (action, contributor, lamports);
                env::log_str("MPC signature request failed");
                None
            }
        }
    }

    /// Callback for the two parallel MPC signs (raise + mint). Either
    /// failure reverts the launch lock nonce so the relayer can retry.
    #[private]
    pub fn on_launch_signatures(
        &mut self,
        trust_id: u64,
        message_hash_hex: String,
        #[callback_result] raise_sig: Result<Value, PromiseError>,
        #[callback_result] mint_sig: Result<Value, PromiseError>,
    ) -> Option<Value> {
        match (raise_sig, mint_sig) {
            (Ok(raise), Ok(mint)) => {
                env::log_str(&format!(
                    "EVENT_JSON:{{\"standard\":\"keyless-trust\",\"version\":\"0.3.0\",\"event\":\"mpc_launch_signatures\",\"data\":[{{\"trust_id\":{trust_id},\"message_hash\":\"{message_hash_hex}\"}}]}}"
                ));
                Some(near_sdk::serde_json::json!({
                    "raise": raise,
                    "mint": mint,
                }))
            }
            _ => {
                if let Some(pending) = self.pending.get(&message_hash_hex).cloned() {
                    self.revert_lock_nonce(&pending);
                    self.clear_pending(&pending);
                }
                env::log_str("MPC launch signatures failed");
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
            distributed_tokens: 0,
            last_solana_sig: String::new(),
        };
        self.ledger.insert(key, row.clone());
        row
    }

    fn view_trust(&self, trust: &Trust) -> TrustView {
        let mut view = TrustView::from(trust);
        view.state = projected_state(trust);
        view
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
            TrustState::Launched | TrustState::Refunding | TrustState::Distributed => {}
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

    fn fee_dest(&self, trust_id: u64) -> [u8; 32] {
        let trust = self.trusts.get(&trust_id).expect("unknown trust");
        let addr = trust
            .fee_address
            .as_ref()
            .expect("fee address not derived; call request_derived_key first");
        bs58::decode_pubkey(addr).unwrap_or_else(|e| env::panic_str(e))
    }

    fn any_pending_sweep(&self, trust_id: u64) -> bool {
        self.ledger.values().any(|row| {
            row.trust_id == trust_id && self.pending_amount(trust_id, &row.contributor, "sweep") > 0
        })
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
                contributor
                    .as_deref()
                    .expect("contributor required for kind=in"),
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
            Some(p) if !p.settled && !p.released => {
                if kind == "distribute" {
                    distribute_leg_amount(&p.extra, contributor)
                } else {
                    p.lamports
                }
            }
            _ => 0,
        }
    }

    fn claimable_of(&self, trust_id: u64, contributor: &str) -> u64 {
        let Some(trust) = self.trusts.get(&trust_id) else {
            return 0;
        };
        if projected_state(trust) != TrustState::Launched {
            return 0;
        }
        let Some(row) = self.ledger.get(&ledger_key(trust_id, contributor)) else {
            return 0;
        };
        let share = pro_rata_share(
            trust.tokens_bought,
            row.credited_lamports,
            trust.credited_total,
        );
        let pending = self.pending_amount(trust_id, contributor, "distribute");
        share
            .saturating_sub(row.distributed_tokens)
            .saturating_sub(pending)
    }

    fn require_distribute_owner(&self, trust_id: u64, owner: &str) {
        require!(
            self.ledger.get(&ledger_key(trust_id, owner)).is_some(),
            "destination owner is not the registered contributor"
        );
    }

    fn distribute_candidates(&self, trust_id: u64) -> Vec<([u8; 32], u64)> {
        self.ledger
            .values()
            .filter(|row| row.trust_id == trust_id && row.credited_lamports > 0)
            .filter_map(|row| {
                let pk = crate::bs58::decode_pubkey(&row.contributor).ok()?;
                Some((pk, row.credited_lamports))
            })
            .collect()
    }

    fn sum_shares(&self, trust_id: u64, tokens: u64, credited_total: u64) -> u64 {
        let mut acc = 0u64;
        for row in self.ledger.values().filter(|row| row.trust_id == trust_id) {
            let share = pro_rata_share(tokens, row.credited_lamports, credited_total);
            acc = acc.checked_add(share).expect("share sum overflow");
        }
        require!(acc <= tokens, "share sum exceeds tokens bought");
        acc
    }

    fn lock_pending(
        &mut self,
        trust_id: u64,
        contributor: &str,
        kind: &str,
        lamports: u64,
        message: &[u8],
        extra: String,
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
        let lk = lock_key(trust_id, contributor, kind);
        self.supersede_released(&lk);
        let nonce = self
            .lock_nonces
            .get(&lk)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.lock_nonces.insert(lk.clone(), nonce);
        let pending = PendingTransfer {
            trust_id,
            contributor: contributor.to_string(),
            kind: kind.to_string(),
            lamports,
            message_hash: message_hash.clone(),
            expires_at_ns: env::block_timestamp().saturating_add(PENDING_EXPIRY_NS),
            settled: false,
            released: false,
            nonce,
            superseded: false,
            extra,
        };
        self.pending.insert(message_hash.clone(), pending);
        self.locks.insert(lk, message_hash);
    }

    fn lock_distribute(&mut self, trust_id: u64, legs: &[(String, u64)], message: &[u8]) {
        require!(!legs.is_empty(), "message has no instructions");
        for (contributor, _) in legs {
            require!(
                self.pending_amount(trust_id, contributor, "distribute") == 0,
                "transfer already pending"
            );
        }
        let message_hash = hex::encode(env::sha256(message));
        require!(
            self.consumed_hashes.get(&message_hash).is_none(),
            "message already settled"
        );
        require!(
            self.pending.get(&message_hash).is_none(),
            "message hash already pending"
        );
        let mut stored = Vec::with_capacity(legs.len());
        for (contributor, amount) in legs {
            let lk = lock_key(trust_id, contributor, "distribute");
            self.supersede_released(&lk);
            let nonce = self
                .lock_nonces
                .get(&lk)
                .copied()
                .unwrap_or(0)
                .saturating_add(1);
            self.lock_nonces.insert(lk, nonce);
            stored.push(DistPendingLeg {
                contributor: contributor.clone(),
                amount: *amount,
                nonce,
            });
        }
        let extra = format_distribute_extra(&stored);
        let total = legs.iter().map(|(_, a)| *a).sum();
        let pending = PendingTransfer {
            trust_id,
            contributor: stored[0].contributor.clone(),
            kind: "distribute".to_string(),
            lamports: total,
            message_hash: message_hash.clone(),
            expires_at_ns: env::block_timestamp().saturating_add(PENDING_EXPIRY_NS),
            settled: false,
            released: false,
            nonce: stored[0].nonce,
            superseded: false,
            extra,
        };
        self.pending.insert(message_hash.clone(), pending);
        for leg in &stored {
            self.locks.insert(
                lock_key(trust_id, &leg.contributor, "distribute"),
                message_hash.clone(),
            );
        }
    }

    fn supersede_released(&mut self, lk: &str) {
        let Some(old) = self.last_released.get(lk).cloned() else {
            return;
        };
        if let Some(mut prev) = self.pending.get(&old).cloned() {
            prev.superseded = true;
            self.pending.insert(old, prev);
        }
    }

    fn revert_lock_nonce(&mut self, pending: &PendingTransfer) {
        let legs = if pending.kind == "distribute" {
            parse_distribute_extra(&pending.extra)
                .into_iter()
                .map(|leg| (leg.contributor, leg.nonce))
                .collect()
        } else {
            vec![(pending.contributor.clone(), pending.nonce)]
        };
        for (contributor, nonce) in legs {
            let lk = lock_key(pending.trust_id, &contributor, &pending.kind);
            if self.lock_nonces.get(&lk).copied() == Some(nonce) {
                self.lock_nonces.insert(lk.clone(), nonce.saturating_sub(1));
            }
            if let Some(old) = self.last_released.get(&lk).cloned() {
                if let Some(mut prev) = self.pending.get(&old).cloned() {
                    prev.superseded = false;
                    self.pending.insert(old, prev);
                }
            }
        }
    }

    fn remaining_budget(&self, trust_id: u64, contributor: &str, kind: &str) -> u64 {
        match kind {
            "sweep" => self.sweepable(trust_id, contributor.to_string()),
            "refund" => self.refundable(trust_id, contributor.to_string()),
            "raise_refund" => self.raise_refundable(trust_id, contributor.to_string()),
            "fill_fee" => {
                let trust = self.trusts.get(&trust_id).expect("unknown trust");
                fill_fee_lamports(trust.credited_total, trust.fill_fee_bps)
                    .saturating_sub(trust.fill_fee_paid)
            }
            "launch" => {
                let trust = self.trusts.get(&trust_id).expect("unknown trust");
                if projected_state(trust) != TrustState::Executing || trust.pool_address.is_some() {
                    0
                } else {
                    let fee = fill_fee_lamports(trust.credited_total, trust.fill_fee_bps);
                    trust.credited_total.saturating_sub(fee)
                }
            }
            _ => 0,
        }
    }

    fn settle_pending(&mut self, pending: &PendingTransfer, solana_sig: &str) {
        match pending.kind.as_str() {
            "sweep" | "refund" | "raise_refund" => {
                let key = ledger_key(pending.trust_id, &pending.contributor);
                let mut credit_claw = 0u64;
                if pending.kind == "refund" {
                    let state = self
                        .trusts
                        .get(&pending.trust_id)
                        .map(projected_state)
                        .expect("unknown trust");
                    if let Some(row) = self.ledger.get(&key) {
                        let base = refundable_lamports(
                            &state,
                            row.credited_lamports,
                            row.overflow_lamports,
                            row.swept_lamports,
                            row.refunded_lamports,
                        );
                        credit_claw = pending.lamports.saturating_sub(base);
                    }
                }
                let row = self.ledger.get_mut(&key).expect("unknown contributor");
                match pending.kind.as_str() {
                    "sweep" => {
                        row.swept_lamports = row.swept_lamports.saturating_add(pending.lamports)
                    }
                    "refund" => {
                        row.refunded_lamports =
                            row.refunded_lamports.saturating_add(pending.lamports);
                        if credit_claw > 0 {
                            row.credited_lamports =
                                row.credited_lamports.saturating_sub(credit_claw);
                        }
                    }
                    "raise_refund" => {
                        row.raise_refunded_lamports =
                            row.raise_refunded_lamports.saturating_add(pending.lamports)
                    }
                    _ => unreachable!(),
                }
                row.last_solana_sig = solana_sig.to_string();
                if credit_claw > 0 {
                    if let Some(trust) = self.trusts.get_mut(&pending.trust_id) {
                        trust.credited_total = trust.credited_total.saturating_sub(credit_claw);
                    }
                }
            }
            "fill_fee" => {
                if let Some(trust) = self.trusts.get_mut(&pending.trust_id) {
                    trust.fill_fee_paid = trust.fill_fee_paid.saturating_add(pending.lamports);
                }
            }
            "launch" => {
                let (curve, stored, credited_total) = {
                    let trust = self.trusts.get(&pending.trust_id).expect("unknown trust");
                    (
                        trust.curve.clone(),
                        trust.tokens_bought,
                        trust.credited_total,
                    )
                };
                let quoted =
                    quote_first_buy(&curve, pending.lamports).unwrap_or_else(|e| env::panic_str(e));
                require!(
                    quoted == stored && stored > 0 && stored <= MAX_TOKENS_BOUGHT,
                    "quoted output does not match stored basis"
                );
                let claimable_total = self.sum_shares(pending.trust_id, stored, credited_total);
                if let Some(trust) = self.trusts.get_mut(&pending.trust_id) {
                    trust.tokens_bought = stored;
                    trust.claimable_total = claimable_total;
                    trust.distributed_total = 0;
                    trust.state = if claimable_total == 0 {
                        TrustState::Distributed
                    } else {
                        TrustState::Launched
                    };
                    let (pool, fee_in_tx) = parse_launch_extra(&pending.extra);
                    if let Some(pool) = pool {
                        trust.pool_address = Some(pool);
                    }
                    trust.fill_fee_paid = trust.fill_fee_paid.saturating_add(fee_in_tx);
                }
            }
            "distribute" => {
                let legs = parse_distribute_extra(&pending.extra);
                let mut booked = 0u64;
                for leg in &legs {
                    let key = ledger_key(pending.trust_id, &leg.contributor);
                    let row = self.ledger.get_mut(&key).expect("unknown contributor");
                    row.distributed_tokens = row.distributed_tokens.saturating_add(leg.amount);
                    row.last_solana_sig = solana_sig.to_string();
                    booked = booked.saturating_add(leg.amount);
                }
                if let Some(trust) = self.trusts.get_mut(&pending.trust_id) {
                    trust.distributed_total = trust.distributed_total.saturating_add(booked);
                    if trust.distributed_total >= trust.claimable_total && trust.claimable_total > 0
                    {
                        trust.state = TrustState::Distributed;
                    }
                }
            }
            _ => env::panic_str("unknown pending kind"),
        }
        self.seen_sigs
            .insert(solana_sig.to_string(), pending.trust_id);
        self.consumed_hashes
            .insert(pending.message_hash.clone(), pending.trust_id);
        self.pending.remove(&pending.message_hash);
        self.clear_lock(pending);
        let lock_ids = if pending.kind == "distribute" {
            parse_distribute_extra(&pending.extra)
                .into_iter()
                .map(|leg| lock_key(pending.trust_id, &leg.contributor, "distribute"))
                .collect()
        } else {
            vec![lock_key(
                pending.trust_id,
                &pending.contributor,
                &pending.kind,
            )]
        };
        for lk in lock_ids {
            if self
                .last_released
                .get(&lk)
                .map(|h| h == &pending.message_hash)
                .unwrap_or(false)
            {
                self.last_released.remove(&lk);
            }
        }
    }

    fn flush_store(&mut self) {
        self.trusts.flush();
        self.ledger.flush();
        self.seen_sigs.flush();
        self.pending.flush();
        self.locks.flush();
        self.consumed_hashes.flush();
        self.lock_nonces.flush();
        self.last_released.flush();
    }

    fn storage_snapshot(&mut self) -> u64 {
        self.flush_store();
        env::storage_usage()
    }

    fn require_watcher(&self, trust_id: u64) {
        let watcher = self
            .trusts
            .get(&trust_id)
            .expect("unknown trust")
            .watcher
            .clone();
        require!(
            env::predecessor_account_id() == watcher,
            "only the trusted watcher can request this sign"
        );
    }

    fn charge_storage(&mut self, before: u64, min_deposit: NearToken) {
        self.flush_store();
        let used = env::storage_usage().saturating_sub(before) as u128;
        let measured = used.saturating_mul(env::storage_byte_cost().as_yoctonear());
        let required = measured.max(min_deposit.as_yoctonear());
        let attached = env::attached_deposit().as_yoctonear();
        require!(attached >= required, "insufficient storage deposit");
        let refund = attached.saturating_sub(required);
        if refund > 0 {
            let _ = Promise::new(env::predecessor_account_id())
                .transfer(NearToken::from_yoctonear(refund));
        }
    }

    fn clear_lock(&mut self, pending: &PendingTransfer) {
        let keys = if pending.kind == "distribute" {
            parse_distribute_extra(&pending.extra)
                .into_iter()
                .map(|leg| lock_key(pending.trust_id, &leg.contributor, "distribute"))
                .collect()
        } else {
            vec![lock_key(
                pending.trust_id,
                &pending.contributor,
                &pending.kind,
            )]
        };
        for lk in keys {
            if self
                .locks
                .get(&lk)
                .map(|h| h == &pending.message_hash)
                .unwrap_or(false)
            {
                self.locks.remove(&lk);
            }
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

    fn sign_launch_tx(
        &self,
        raise_path: String,
        mint_path: String,
        trust_id: u64,
        message: &[u8],
    ) -> Promise {
        let payload_hex = hex::encode(message);
        let message_hash_hex = hex::encode(env::sha256(message));
        sign_eddsa_promise(self.mpc_contract.clone(), raise_path, payload_hex.clone())
            .and(sign_eddsa_promise(
                self.mpc_contract.clone(),
                mint_path,
                payload_hex,
            ))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(MPC_CALLBACK_GAS)
                    .on_launch_signatures(trust_id, message_hash_hex),
            )
    }
}

struct DistPendingLeg {
    contributor: String,
    amount: u64,
    nonce: u64,
}

fn format_distribute_extra(legs: &[DistPendingLeg]) -> String {
    legs.iter()
        .map(|leg| format!("{}:{}:{}", leg.contributor, leg.amount, leg.nonce))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_distribute_extra(extra: &str) -> Vec<DistPendingLeg> {
    require!(!extra.is_empty(), "bad distribute pending");
    extra
        .split(',')
        .map(|part| {
            let mut it = part.split(':');
            let contributor = it.next().unwrap_or("").to_string();
            let amount = it
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| env::panic_str("bad distribute pending"));
            let nonce = it
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| env::panic_str("bad distribute pending"));
            require!(it.next().is_none(), "bad distribute pending");
            require!(!contributor.is_empty(), "bad distribute pending");
            DistPendingLeg {
                contributor,
                amount,
                nonce,
            }
        })
        .collect()
}

fn distribute_leg_amount(extra: &str, contributor: &str) -> u64 {
    parse_distribute_extra(extra)
        .into_iter()
        .find(|leg| leg.contributor == contributor)
        .map(|leg| leg.amount)
        .unwrap_or(0)
}

fn parse_launch_extra(extra: &str) -> (Option<String>, u64) {
    let mut parts = extra.splitn(2, ':');
    let pool = parts
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let fee = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (pool, fee)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::RENT_EXEMPT_MIN;
    use crate::shapes::SYSTEM_PROGRAM;
    use crate::solana::message::{
        serialize_legacy_message, CompiledInstruction, LegacyMessage, MessageHeader,
    };
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    const DEMO_PUBKEY: &str = "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj";
    const DEPOSIT_ADDR: &str = "F1aZA8xz3ueSpV5CQFB8PGz8y5QXZZtFdZJRTcZQ8DR6";
    const RAISE_ADDR: &str = "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj";
    const OTHER_VAULT: &str = "7bjnioap6J85RWuqPn2CEFRJSSMGvm6CGDR5SsxvxfLa";
    const SIG_A: &str =
        "32cEnng3NKyCv2VLQ47M2uXnVDp1LZH7B7rtWZtzHDP2QHz6yrEbFJw7kyWerMs1CYZ2vMoa3cgrcKmKkGNgbd9G";
    const SIG_B: &str =
        "2wBCwpDikerJ7WiYNu4pRGkyP2QTD5EvHg6mFgnvyw2Q291M72Lpj542ZXnAcxewghaGo14cEg6KHiRZKjtVTMza";
    const SIG_C: &str =
        "2UwjXHH8ochhUvGhnDx7pSYDwJWUBdsLkav6yEShNXjqWNqMafUTqvU8fu23QEUKWErME8bqY42cvQoU9XEPWr8A";
    const SIG_D: &str =
        "452rHV4vsvxXjAYKXceyeQYmiavoLfAtBbj14kWi9Xdr6S6NCWbRtN9a6WsCi5NLprEiBSeBXdNLhVQKezo6pr9e";
    const PAID: NearToken = NearToken::from_millinear(10);

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
            cap_bps: 100,
            fill_fee_bps: Some(150),
            execution_timeout_ns: Some(PENDING_EXPIRY_NS.saturating_mul(2)),
            watcher: accounts(2),
            curve: crate::quote::demo_curve(),
        }
    }

    /// 100% cap so a single test wallet can fill the raise.
    fn fill_config(target: u64, deadline_ns: u64) -> CreateTrustConfig {
        let mut cfg = demo_config(target, deadline_ns);
        cfg.cap_bps = 10_000;
        cfg
    }

    fn fill_config_short_timeout(target: u64, deadline_ns: u64) -> CreateTrustConfig {
        let mut cfg = fill_config(target, deadline_ns);
        cfg.execution_timeout_ns = Some(1_000);
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

    fn open_fillable_short(c: &mut Contract, target: u64, deadline_ns: u64) -> u64 {
        c.create_trust(fill_config_short_timeout(target, deadline_ns))
    }

    fn mint_bytes() -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = 42;
        k[31] = 42;
        k
    }

    fn mint_addr() -> String {
        crate::bs58::encode(&mint_bytes())
    }

    fn pool_bytes() -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = 77;
        k[31] = 77;
        k
    }

    fn config_bytes() -> [u8; 32] {
        bs58::decode_pubkey("Config1111111111111111111111111111111111111").unwrap()
    }

    fn fee_bytes() -> [u8; 32] {
        bs58::decode_pubkey(OTHER_VAULT).unwrap()
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

    fn seed_mint(c: &mut Contract, id: u64) {
        let _ = c.on_derived_public_key(
            id,
            "mint".into(),
            None,
            Ok(format!("ed25519:{}", mint_addr())),
        );
    }

    fn quoted_out(buy: u64) -> u64 {
        quote_first_buy(&crate::quote::demo_curve(), buy).expect("quote")
    }

    fn launch_bytes(buy: u64, fee: u64) -> Vec<u8> {
        crate::shapes::dbc::tests::fixture_launch_bytes(
            [8u8; 32],
            raise_bytes(),
            mint_bytes(),
            fee_bytes(),
            config_bytes(),
            pool_bytes(),
            buy,
            fee,
            quoted_out(buy),
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        )
    }

    fn go_executing_swept(
        c: &mut Contract,
        id: u64,
        amount: u64,
        report_sig: &str,
        confirm_sig: &str,
    ) {
        seed_raise(c, id);
        seed_mint(c, id);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), amount, report_sig.into());
        seed_deposit(c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), amount);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&bytes), confirm_sig.into());
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
        assert_eq!(trust.mint_path, "trust/1/mint");
        assert_eq!(trust.cap_bps, 100);
        assert_eq!(trust.fill_fee_bps, 150);
        assert_eq!(trust.params_hash.len(), 64);
        assert_eq!(trust.fill_fee_paid, 0);
        assert!(trust.pool_address.is_none());
        assert_eq!(c.fill_fee(id), 0);
        assert_eq!(c.launch_attach_tgas(), 90);
        assert_eq!(c.mpc_contract().as_str(), DEFAULT_MPC);
        let fixture: near_sdk::serde_json::Value =
            near_sdk::serde_json::from_str(include_str!("../tests/fixtures/dbc_quotes.json"))
                .unwrap();
        assert_eq!(
            trust.params_hash,
            fixture["demoParamsHash"].as_str().unwrap()
        );
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
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 5, SIG_A.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 5);
    }

    #[test]
    fn execution_timeout_from_filled() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable_short(&mut c, 10, 50_000);
        testing_env!(context_at(accounts(2), PAID, 1_000).build());
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
        testing_env!(context(accounts(2), PAID).build());
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c.report_deposit(id, DEMO_PUBKEY.into(), 1, SIG_A.into());
        }));
        assert!(panicked.is_err());
        testing_env!(context(accounts(3), PAID).build());
        let row = c.report_deposit(id, DEMO_PUBKEY.into(), 1, SIG_B.into());
        assert_eq!(row.credited_lamports, 1);
    }

    #[test]
    fn duplicate_sig_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let attacker = [9u8; 32];
        let bytes = transfer_bytes(deposit_bytes(), attacker, 10);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer destination is not the fixed owner")]
    fn request_sweep_rejects_attacker_destination() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), [9u8; 32], 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    fn request_sweep_accepts_relayer_paid_raise_transfer() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .refunded_lamports,
            0
        );
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 10);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    fn late_deposit_after_fill_is_all_overflow() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_C.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_refund_rejects_fee_payer_as_source() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let fee_payer = [8u8; 32];
        let bytes = relayer_paid_bytes(fee_payer, fee_payer, owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_refund_rejects_other_contributor_vault() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let other = bs58::decode_pubkey(OTHER_VAULT).unwrap();
        let bytes = relayer_paid_bytes([8u8; 32], other, owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer already pending")]
    fn double_sign_is_blocked_while_pending() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 6);
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), raise_bytes(), 4);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(first));
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(second));
    }

    #[test]
    fn expired_pending_can_be_released() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        c.release_pending(message_hash(&bytes));
    }

    #[test]
    fn request_raise_refund_from_raise_vault() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable_short(&mut c, 10, 2_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let sweep = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(sweep.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&sweep), SIG_B.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        assert_eq!(c.raise_refundable(id, DEMO_PUBKEY.into()), 10);
        let refund = relayer_paid_bytes([8u8; 32], raise_bytes(), owner_bytes(), 10);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_raise_refund(id, DEMO_PUBKEY.into(), Base64VecU8(refund.clone()));
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        testing_env!(context_at(accounts(2), PAID, 1_000 + PENDING_EXPIRY_NS + 1).build());
        c.confirm_transfer(hash.clone(), SIG_D.into());
        assert!(c.get_pending(hash.clone()).is_none());
        assert!(c.is_consumed(hash));
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        testing_env!(context_at(
            accounts(1),
            NearToken::from_yoctonear(0),
            1_000 + PENDING_EXPIRY_NS + 1
        )
        .build());
        c.release_pending(hash.clone());
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(first));
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context_at(accounts(2), PAID, 1_000 + PENDING_EXPIRY_NS + 1).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(second));
    }

    #[test]
    fn overflow_refund_in_filled() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        c.report_deposit(id, DEMO_PUBKEY.into(), 7, SIG_B.into());
        seed_deposit(&mut c, id);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Filled);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 7);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 7);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), PAID).build());
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
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(1), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&bytes), SIG_D.into());
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_raise_refund_rejects_deposit_as_source() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable_short(&mut c, 10, 2_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let sweep = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(sweep.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&sweep), SIG_B.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        let refund = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 10);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_raise_refund(id, DEMO_PUBKEY.into(), Base64VecU8(refund));
    }

    #[test]
    #[should_panic(expected = "deposit address not derived")]
    fn request_sweep_rejects_when_deposit_address_missing() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn request_sweep_rejects_fee_payer_as_source() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let fee_payer = [8u8; 32];
        let bytes = relayer_paid_bytes(fee_payer, fee_payer, raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    fn expire_and_release(c: &mut Contract, hash: &str) {
        testing_env!(context_at(
            accounts(1),
            NearToken::from_yoctonear(0),
            1_000 + PENDING_EXPIRY_NS + 1
        )
        .build());
        c.release_pending(hash.to_string());
    }

    #[test]
    #[should_panic(expected = "pending superseded")]
    fn resign_then_late_confirm_same_sig_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(first.clone()));
        let first_hash = message_hash(&first);
        expire_and_release(&mut c, &first_hash);
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(second.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&second), SIG_D.into());
        c.confirm_transfer(first_hash, SIG_D.into());
    }

    #[test]
    #[should_panic(expected = "pending superseded")]
    fn late_confirm_superseded_with_unused_sig_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(first.clone()));
        let first_hash = message_hash(&first);
        expire_and_release(&mut c, &first_hash);
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(second.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&second), SIG_D.into());
        c.confirm_transfer(first_hash, SIG_C.into());
    }

    #[test]
    fn honest_late_confirm_without_resign_still_books() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        expire_and_release(&mut c, &hash);
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(hash.clone(), SIG_D.into());
        assert!(c.is_consumed(hash));
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .swept_lamports,
            10
        );
        assert_eq!(c.pending_expiry_ns(), PENDING_EXPIRY_NS);
    }

    #[test]
    #[should_panic(expected = "nothing to refund")]
    fn refunding_cannot_refund_while_sweep_pending() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable_short(&mut c, 10, 2_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let sweep = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(sweep));
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 0);
        let refund = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 10);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(refund));
    }

    #[test]
    #[should_panic(expected = "insufficient storage deposit")]
    fn register_rejects_insufficient_deposit() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), ONE_YOCTO).build());
        let _ = c.register(id, DEMO_PUBKEY.into());
    }

    #[test]
    fn register_with_exact_register_deposit_succeeds() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), c.register_deposit()).build());
        let path = c.register(id, DEMO_PUBKEY.into());
        assert_eq!(path, format!("trust/1/in/{DEMO_PUBKEY}"));
    }

    #[test]
    fn sign_with_exact_sign_deposit_succeeds() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), c.sign_deposit()).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        assert!(c.get_pending(message_hash(&bytes)).is_some());
    }

    #[test]
    fn sweepable_ignores_pending_overflow_refund() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        c.report_deposit(id, DEMO_PUBKEY.into(), 7, SIG_B.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Filled);
        assert_eq!(c.sweepable(id, DEMO_PUBKEY.into()), 10);
        let refund = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 7);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(refund));
        assert_eq!(c.sweepable(id, DEMO_PUBKEY.into()), 10);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 0);
    }

    #[test]
    fn views_apply_timeout_without_try_timeout() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 2_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 5, SIG_A.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Refunding);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 5);
    }

    #[test]
    #[should_panic(expected = "pending superseded")]
    fn late_confirm_after_state_change_and_partial_resign_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 2_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(first.clone()));
        let first_hash = message_hash(&first);
        expire_and_release(&mut c, &first_hash);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Refunding);
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), owner_bytes(), 50);
        testing_env!(context_at(accounts(1), PAID, 1_000 + PENDING_EXPIRY_NS + 1).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(second.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&second), SIG_D.into());
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .refunded_lamports,
            50
        );
        c.confirm_transfer(first_hash, SIG_C.into());
    }

    #[test]
    #[should_panic(expected = "pending superseded")]
    fn late_confirm_after_partial_resign_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(first.clone()));
        let first_hash = message_hash(&first);
        expire_and_release(&mut c, &first_hash);
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(second.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&second), SIG_D.into());
        c.confirm_transfer(first_hash, SIG_C.into());
    }

    #[test]
    fn superseded_entry_is_marked_on_resign() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let first = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(first.clone()));
        let first_hash = message_hash(&first);
        expire_and_release(&mut c, &first_hash);
        assert!(!c.get_pending(first_hash.clone()).unwrap().superseded);
        let second = relayer_paid_bytes([7u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(second));
        assert!(c.get_pending(first_hash).unwrap().superseded);
    }

    const SIG_E: &str =
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    const SIG_F: &str =
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    #[test]
    fn request_launch_after_sweep_moves_to_launched() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Executing);
        assert_eq!(c.swept_total(id), 1_000);
        assert_eq!(c.fill_fee(id), 15);
        let bytes = launch_bytes(985, 15);
        assert!(bytes.len() < 1232);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        let pending = c.get_pending(hash.clone()).unwrap();
        assert_eq!(pending.kind, "launch");
        assert_eq!(pending.lamports, 985);
        let quoted = quoted_out(985);
        assert_eq!(c.get_trust(id).unwrap().tokens_bought, quoted);
        assert_eq!(c.get_trust(id).unwrap().claimable_total, 0);
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_launch_outcome(hash, SIG_E.into());
        let trust = c.get_trust(id).unwrap();
        assert_eq!(trust.state, TrustState::Launched);
        assert_eq!(trust.tokens_bought, quoted);
        assert_eq!(trust.claimable_total, quoted);
        assert_eq!(trust.distributed_total, 0);
        assert_eq!(
            trust.pool_address.as_deref(),
            Some(
                crate::bs58::encode(&crate::shapes::dbc::tests::fixture_derived_pool(
                    config_bytes(),
                    mint_bytes(),
                    raise_bytes(),
                ))
                .as_str()
            )
        );
        assert_eq!(trust.fill_fee_paid, 15);
        assert_eq!(trust.mint_address.as_deref(), Some(mint_addr().as_str()));
    }

    #[test]
    #[should_panic(expected = "minimum_amount_out is not the quoted first-buy output")]
    fn request_launch_rejects_min_out_one() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let bytes = crate::shapes::dbc::tests::fixture_launch_bytes(
            [8u8; 32],
            raise_bytes(),
            mint_bytes(),
            fee_bytes(),
            config_bytes(),
            pool_bytes(),
            985,
            15,
            1,
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        );
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "minimum_amount_out is not the quoted first-buy output")]
    fn request_launch_rejects_min_out_off_by_one() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let bytes = crate::shapes::dbc::tests::fixture_launch_bytes(
            [8u8; 32],
            raise_bytes(),
            mint_bytes(),
            fee_bytes(),
            config_bytes(),
            pool_bytes(),
            985,
            15,
            quoted_out(985) + 1,
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        );
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "sweep not settled")]
    fn request_launch_rejects_before_sweep_settled() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        seed_raise(&mut c, id);
        seed_mint(&mut c, id);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 1_000, SIG_A.into());
        seed_deposit(&mut c, id);
        let sweep = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 1_000);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(sweep));
        let bytes = launch_bytes(985, 15);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "launch only in Executing")]
    fn request_launch_rejects_wrong_state() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        seed_raise(&mut c, id);
        seed_mint(&mut c, id);
        let bytes = launch_bytes(985, 15);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "launch instruction account does not match the derived pin")]
    fn request_launch_rejects_tampered_config() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let mut bad_config = [0u8; 32];
        bad_config[0] = 9;
        bad_config[31] = 9;
        let bytes = crate::shapes::dbc::tests::fixture_launch_bytes(
            [8u8; 32],
            raise_bytes(),
            mint_bytes(),
            fee_bytes(),
            bad_config,
            pool_bytes(),
            985,
            15,
            quoted_out(985),
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        );
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    fn request_fill_fee_then_launch_without_in_tx_fee() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let fee_tx = relayer_paid_bytes([8u8; 32], raise_bytes(), fee_bytes(), 15);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_fill_fee(id, Base64VecU8(fee_tx.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&fee_tx), SIG_C.into());
        assert_eq!(c.get_trust(id).unwrap().fill_fee_paid, 15);
        let full = launch_bytes(985, 15);
        let mut parsed = parse_legacy_message(&full).unwrap();
        parsed.instructions.remove(0);
        let paid = serialize_legacy_message(&parsed);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(paid.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_launch_outcome(message_hash(&paid), SIG_F.into());
        let trust = c.get_trust(id).unwrap();
        assert_eq!(trust.state, TrustState::Launched);
        assert_eq!(trust.fill_fee_paid, 15);
        assert!(trust.pool_address.is_some());
    }

    fn pk(tag: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = tag;
        k[31] = tag;
        k
    }

    fn pk_b58(tag: u8) -> String {
        crate::bs58::encode(&pk(tag))
    }

    fn seed_in(c: &mut Contract, id: u64, contributor: &str, addr: &str) {
        let _ = c.on_derived_public_key(
            id,
            "in".into(),
            Some(contributor.into()),
            Ok(format!("ed25519:{addr}")),
        );
    }

    /// Credited contributors, sweeps settled, launch confirmed.
    /// Credits are `amounts` (must sum to `target`). The share basis is the
    /// contract quote of `target − fill fee`, not a watcher balance.
    fn go_launched_split(
        c: &mut Contract,
        target: u64,
        amounts: &[(u8, u64)],
        with_fee: bool,
    ) -> u64 {
        let id = c.create_trust(fill_config(target, 9_000));
        seed_raise(c, id);
        seed_mint(c, id);
        testing_env!(context(accounts(1), c.register_deposit()).build());
        let _ = c.register(id, pk_b58(13));
        let mut reported = Vec::new();
        for (i, (tag, amount)) in amounts.iter().enumerate() {
            let contributor = pk_b58(*tag);
            let deposit = pk_b58(100 + *tag);
            let report_sig = format!("{:0>88}", (i as u8) + 1);
            testing_env!(context(accounts(2), PAID).build());
            c.report_deposit(id, contributor.clone(), *amount, report_sig);
            seed_in(c, id, &contributor, &deposit);
            reported.push((*tag, *amount, contributor));
        }
        for (i, (tag, amount, contributor)) in reported.iter().enumerate() {
            let bytes = relayer_paid_bytes([8u8; 32], pk(100 + *tag), raise_bytes(), *amount);
            testing_env!(context(accounts(2), PAID).build());
            let _ = c.request_sweep(id, contributor.clone(), Base64VecU8(bytes.clone()));
            testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
            let confirm_sig = format!("{:0>86}{}S", i + 1, "C");
            c.confirm_transfer(message_hash(&bytes), confirm_sig);
        }
        let fee = c.fill_fee(id);
        let _ = with_fee;
        let buy = target - fee;
        let bytes = crate::shapes::dbc::tests::fixture_launch_bytes(
            [8u8; 32],
            raise_bytes(),
            mint_bytes(),
            fee_bytes(),
            config_bytes(),
            pool_bytes(),
            buy,
            fee,
            quoted_out(buy),
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        );
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_launch_outcome(message_hash(&bytes), format!("{:0>88}", 90u8));
        id
    }

    fn dist_bytes(owners: &[([u8; 32], u64)], create: bool) -> Vec<u8> {
        let legs: Vec<([u8; 32], u64, bool)> =
            owners.iter().map(|(o, a)| (*o, *a, create)).collect();
        crate::shapes::build_distribute_message(
            [8u8; 32],
            raise_bytes(),
            mint_bytes(),
            &legs,
            pk(9),
        )
    }

    #[test]
    fn honest_single_claim_books_on_confirm_and_completes() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = go_launched_split(&mut c, 1_000, &[(11, 1_000)], true);
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Launched);
        let owner = pk_b58(11);
        let share = c.claimable(id, owner.clone());
        assert_eq!(share, c.get_trust(id).unwrap().tokens_bought);
        assert!(share > 0);
        let bytes = dist_bytes(&[(pk(11), share)], true);
        assert!(bytes.len() <= 1232);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(bytes.clone()));
        assert_eq!(
            c.get_contribution(id, owner.clone())
                .unwrap()
                .distributed_tokens,
            0
        );
        assert_eq!(c.claimable(id, owner.clone()), 0);
        assert_eq!(c.get_trust(id).unwrap().distributed_total, 0);
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&bytes), format!("{:0>88}", 91u8));
        assert_eq!(
            c.get_contribution(id, owner.clone())
                .unwrap()
                .distributed_tokens,
            share
        );
        let trust = c.get_trust(id).unwrap();
        assert_eq!(trust.state, TrustState::Distributed);
        assert_eq!(trust.distributed_total, share);
        assert_eq!(c.distribution_status(id).dust_tokens, 0);
        assert_eq!(c.claimable(id, owner), 0);
    }

    #[test]
    fn honest_batch_pays_both_and_records_dust() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        // credits 1 and 2 of target 3. Fill fee is 0, so the buy is 3 lamports
        // and the basis is the quote of that buy. Dust is strictly less than
        // the number of credited contributors.
        let id = go_launched_split(&mut c, 3, &[(11, 1), (12, 2)], false);
        let a = c.claimable(id, pk_b58(11));
        let b = c.claimable(id, pk_b58(12));
        let status = c.distribution_status(id);
        assert_eq!(a, pro_rata_share(status.tokens_bought, 1, 3));
        assert_eq!(b, pro_rata_share(status.tokens_bought, 2, 3));
        assert!(status.dust_tokens < 2);
        assert_eq!(status.decimals, 6);
        assert_eq!(status.claimable_total, a + b);
        assert!(status.claimable_total <= status.tokens_bought);
        let bytes = dist_bytes(&[(pk(11), a), (pk(12), b)], true);
        testing_env!(context(accounts(4), c.sign_deposit()).build());
        let _ = c.request_distribute(id, Base64VecU8(bytes.clone()));
        // The other contributor is inside the batch, so a second claim is locked.
        assert_eq!(c.claimable(id, pk_b58(12)), 0);
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&bytes), format!("{:0>88}", 92u8));
        let trust = c.get_trust(id).unwrap();
        assert_eq!(trust.state, TrustState::Distributed);
        assert_eq!(trust.distributed_total, a + b);
        assert_eq!(
            c.get_contribution(id, pk_b58(11))
                .unwrap()
                .distributed_tokens,
            a
        );
        assert_eq!(
            c.get_contribution(id, pk_b58(12))
                .unwrap()
                .distributed_tokens,
            b
        );
    }

    #[test]
    fn over_reported_basis_is_impossible_and_the_last_claimer_is_paid() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        // 30/60/10 of a 1e9 raise. A 1.5× basis lets the 30% claimer take
        // more than the raise holds, and the 60% claimer is left short.
        // The contract basis is the quote of 985_000_000, so that report
        // cannot be stored.
        let id = go_launched_split(
            &mut c,
            1_000_000_000,
            &[(11, 300_000_000), (12, 600_000_000), (14, 100_000_000)],
            true,
        );
        let trust = c.get_trust(id).unwrap();
        let q = trust.tokens_bought;
        assert_eq!(q, 241_421_378_472_608);
        let inflated = q * 3 / 2;
        let early = pro_rata_share(inflated, 300_000_000, 1_000_000_000);
        let fair60 = pro_rata_share(q, 600_000_000, 1_000_000_000);
        assert!(q - early < fair60);
        let s30 = c.claimable(id, pk_b58(11));
        let s60 = c.claimable(id, pk_b58(12));
        let s10 = c.claimable(id, pk_b58(14));
        assert_eq!(s30, pro_rata_share(q, 300_000_000, 1_000_000_000));
        assert_eq!(s60, fair60);
        assert_eq!(s10, pro_rata_share(q, 100_000_000, 1_000_000_000));
        assert!(s30 + s60 + s10 <= q);
        assert_eq!(trust.claimable_total, s30 + s60 + s10);
        assert_eq!(
            trust.claimable_total,
            c.distribution_status(id).claimable_total
        );
        assert_eq!(
            q - trust.claimable_total,
            c.distribution_status(id).dust_tokens
        );
        for (i, (tag, share)) in [(11u8, s30), (12, s60), (14, s10)].into_iter().enumerate() {
            let bytes = dist_bytes(&[(pk(tag), share)], true);
            testing_env!(context(accounts(3), PAID).build());
            let _ = c.request_distribute(id, Base64VecU8(bytes.clone()));
            testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
            c.confirm_transfer(message_hash(&bytes), format!("{:0>88}", 80 + i as u8));
        }
        let done = c.get_trust(id).unwrap();
        assert_eq!(done.state, TrustState::Distributed);
        assert_eq!(done.distributed_total, s30 + s60 + s10);
        assert_eq!(done.tokens_bought, q);
        assert_eq!(
            c.get_contribution(id, pk_b58(12))
                .unwrap()
                .distributed_tokens,
            s60
        );
    }

    #[test]
    fn one_contributor_lock_does_not_block_the_other() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = go_launched_split(&mut c, 1_000, &[(11, 600), (12, 400)], true);
        let a = c.claimable(id, pk_b58(11));
        let b = c.claimable(id, pk_b58(12));
        assert_eq!(
            a,
            pro_rata_share(c.get_trust(id).unwrap().tokens_bought, 600, 1_000)
        );
        assert_eq!(
            b,
            pro_rata_share(c.get_trust(id).unwrap().tokens_bought, 400, 1_000)
        );
        let first = dist_bytes(&[(pk(11), a)], false);
        testing_env!(context(accounts(4), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(first));
        let second = dist_bytes(&[(pk(12), b)], true);
        testing_env!(context(accounts(5), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(second.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&second), format!("{:0>88}", 93u8));
        assert_eq!(
            c.get_contribution(id, pk_b58(12))
                .unwrap()
                .distributed_tokens,
            b
        );
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Launched);
    }

    #[test]
    fn late_confirm_without_resign_still_books() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = go_launched_split(&mut c, 1_000, &[(11, 1_000)], true);
        let share = c.claimable(id, pk_b58(11));
        let bytes = dist_bytes(&[(pk(11), share)], true);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(bytes.clone()));
        let hash = message_hash(&bytes);
        expire_and_release(&mut c, &hash);
        testing_env!(context_at(
            accounts(2),
            NearToken::from_yoctonear(1),
            1_000 + PENDING_EXPIRY_NS + 1
        )
        .build());
        c.confirm_transfer(hash, format!("{:0>88}", 94u8));
        assert_eq!(
            c.get_contribution(id, pk_b58(11))
                .unwrap()
                .distributed_tokens,
            share
        );
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Distributed);
    }

    #[test]
    #[should_panic(expected = "pending superseded")]
    fn release_then_resign_rejects_the_old_confirm() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = go_launched_split(&mut c, 1_000, &[(11, 1_000)], true);
        let share = c.claimable(id, pk_b58(11));
        let first = dist_bytes(&[(pk(11), share)], true);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(first.clone()));
        let first_hash = message_hash(&first);
        expire_and_release(&mut c, &first_hash);
        let second = dist_bytes(&[(pk(11), share)], false);
        testing_env!(context_at(accounts(3), PAID, 1_000 + PENDING_EXPIRY_NS + 1).build());
        let _ = c.request_distribute(id, Base64VecU8(second.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&second), format!("{:0>88}", 95u8));
        assert_eq!(
            c.get_contribution(id, pk_b58(11))
                .unwrap()
                .distributed_tokens,
            share
        );
        c.confirm_transfer(first_hash, format!("{:0>88}", 96u8));
    }

    #[test]
    #[should_panic(expected = "distribute only in Launched")]
    fn double_claim_after_confirm_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = go_launched_split(&mut c, 1_000, &[(11, 1_000)], true);
        let share = c.claimable(id, pk_b58(11));
        let bytes = dist_bytes(&[(pk(11), share)], true);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&bytes), format!("{:0>88}", 97u8));
        let again = dist_bytes(&[(pk(11), share)], false);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(again));
    }

    #[test]
    #[should_panic(expected = "distribute only in Launched")]
    fn claim_before_launched_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(vec![1, 2, 3]));
    }

    #[test]
    #[should_panic(expected = "nothing to claim")]
    fn zero_credit_claim_is_rejected() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = go_launched_split(&mut c, 1_000, &[(11, 1_000)], true);
        assert_eq!(
            c.get_contribution(id, pk_b58(13))
                .unwrap()
                .credited_lamports,
            0
        );
        let bytes = dist_bytes(&[(pk(13), 1)], true);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(bytes));
    }

    fn launched_for_rejects() -> (Contract, u64) {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = go_launched_split(&mut c, 1_000, &[(11, 1_000)], true);
        (c, id)
    }

    fn try_dist(bytes: Vec<u8>) {
        let (mut c, id) = launched_for_rejects();
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "destination owner is not the registered contributor")]
    fn rejects_wrong_destination_owner() {
        let bytes = dist_bytes(&[(pk(19), 100)], true);
        try_dist(bytes);
    }

    #[test]
    #[should_panic(expected = "base mint is not the derived trust/<id>/mint key")]
    fn rejects_wrong_mint() {
        let (mut c, id) = launched_for_rejects();
        let mut msg = parse_legacy_message(&dist_bytes(&[(pk(11), 100)], false)).unwrap();
        let mint_idx = msg.instructions[0].accounts[1] as usize;
        msg.account_keys[mint_idx] = pk(6);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(serialize_legacy_message(&msg)));
    }

    #[test]
    #[should_panic(expected = "transfer amount is not the remaining share")]
    fn rejects_over_amount() {
        try_dist(dist_bytes(&[(pk(11), 101)], true));
    }

    #[test]
    #[should_panic(expected = "transfer amount is not the remaining share")]
    fn rejects_under_amount() {
        try_dist(dist_bytes(&[(pk(11), 99)], true));
    }

    #[test]
    #[should_panic(expected = "extra instruction beyond the allowed transfer shape")]
    fn rejects_extra_instruction() {
        let (mut c, id) = launched_for_rejects();
        let mut msg = parse_legacy_message(&dist_bytes(&[(pk(11), 100)], false)).unwrap();
        msg.account_keys.push(SYSTEM_PROGRAM);
        msg.header.num_readonly_unsigned += 1;
        msg.instructions.push(CompiledInstruction {
            program_id_index: (msg.account_keys.len() - 1) as u8,
            accounts: vec![0, 1],
            data: vec![2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
        });
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(serialize_legacy_message(&msg)));
    }

    #[test]
    #[should_panic(expected = "transfer source is not the required vault")]
    fn rejects_wrong_source() {
        let (mut c, id) = launched_for_rejects();
        let mut msg = parse_legacy_message(&dist_bytes(&[(pk(11), 100)], false)).unwrap();
        msg.instructions[0].accounts[0] = 1;
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(serialize_legacy_message(&msg)));
    }

    #[test]
    #[should_panic(expected = "ATA payer must be the relayer fee payer")]
    fn rejects_ata_payer_raise() {
        let (mut c, id) = launched_for_rejects();
        let mut msg = parse_legacy_message(&dist_bytes(&[(pk(11), 100)], true)).unwrap();
        msg.instructions[0].accounts[0] = 1;
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(serialize_legacy_message(&msg)));
    }

    #[test]
    #[should_panic(expected = "trailing bytes after legacy message")]
    fn rejects_trailing_data() {
        let (mut c, id) = launched_for_rejects();
        let mut bytes = dist_bytes(&[(pk(11), 100)], true);
        bytes.push(0);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "instruction data length is not exact")]
    fn rejects_trailing_transfer_data() {
        let (mut c, id) = launched_for_rejects();
        let mut msg = parse_legacy_message(&dist_bytes(&[(pk(11), 100)], false)).unwrap();
        msg.instructions[0].data.push(7);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.request_distribute(id, Base64VecU8(serialize_legacy_message(&msg)));
    }

    #[test]
    #[should_panic(expected = "only a linear fee scheduler is quoted")]
    fn create_trust_rejects_exponential_fee() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let mut cfg = demo_config(1_000, 9_000);
        cfg.curve.base_fee_mode = 1;
        let _ = c.create_trust(cfg);
    }

    #[test]
    #[should_panic(expected = "cap_bps out of range")]
    fn create_trust_rejects_zero_cap() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let mut cfg = demo_config(1_000, 9_000);
        cfg.cap_bps = 0;
        let _ = c.create_trust(cfg);
    }

    #[test]
    #[should_panic(expected = "cap_bps out of range")]
    fn create_trust_rejects_cap_above_10000() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let mut cfg = demo_config(1_000, 9_000);
        cfg.cap_bps = 10_001;
        let _ = c.create_trust(cfg);
    }

    #[test]
    #[should_panic(expected = "cannot register after the trust is closed")]
    fn register_rejects_after_launched() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let bytes = launch_bytes(985, 15);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_launch_outcome(message_hash(&bytes), SIG_E.into());
        assert_eq!(c.get_trust(id).unwrap().state, TrustState::Launched);
        testing_env!(context(accounts(3), PAID).build());
        let _ = c.register(id, DEMO_PUBKEY.into());
    }

    #[test]
    #[should_panic(expected = "cannot register after the trust is closed")]
    fn register_rejects_after_refunding() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 2_000);
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.register(id, DEMO_PUBKEY.into());
    }

    #[test]
    fn refund_dust_closeout_when_leftover_below_rent() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 800_000, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 1_000_000, SIG_A.into());
        seed_deposit(&mut c, id);
        let row = c.get_contribution(id, DEMO_PUBKEY.into()).unwrap();
        assert_eq!(row.credited_lamports, 800_000);
        assert_eq!(row.overflow_lamports, 200_000);
        assert!(800_000 < RENT_EXEMPT_MIN);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 1_000_000);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 1_000_000);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), NearToken::from_yoctonear(1)).build());
        c.confirm_transfer(message_hash(&bytes), SIG_E.into());
        let row = c.get_contribution(id, DEMO_PUBKEY.into()).unwrap();
        assert_eq!(row.refunded_lamports, 1_000_000);
        assert_eq!(row.credited_lamports, 0);
        assert_eq!(c.get_trust(id).unwrap().credited_total, 0);
    }

    #[test]
    #[should_panic(expected = "transfer destination is not the fixed owner")]
    fn request_launch_rejects_extra_raise_transfer() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let mut parsed = parse_legacy_message(&launch_bytes(985, 15)).unwrap();
        let attacker = [9u8; 32];
        parsed.account_keys.push(attacker);
        let attacker_idx = (parsed.account_keys.len() - 1) as u8;
        parsed.instructions.insert(
            1,
            CompiledInstruction {
                program_id_index: parsed
                    .account_keys
                    .iter()
                    .position(|k| k == &SYSTEM_PROGRAM)
                    .unwrap() as u8,
                accounts: vec![1, attacker_idx],
                data: {
                    let mut data = vec![2, 0, 0, 0];
                    data.extend_from_slice(&1u64.to_le_bytes());
                    data
                },
            },
        );
        let bytes = serialize_legacy_message(&parsed);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "launch instruction account does not match the derived pin")]
    fn request_launch_rejects_redirected_swap_output() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let mut parsed = parse_legacy_message(&launch_bytes(985, 15)).unwrap();
        let swap = parsed
            .instructions
            .iter_mut()
            .find(|ix| ix.data.len() >= 8 && ix.data[0..8] == crate::shapes::dbc::SWAP_DISC)
            .unwrap();
        swap.accounts[4] = 3;
        let bytes = serialize_legacy_message(&parsed);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "transfer is missing from/to accounts")]
    fn request_launch_rejects_ata_with_too_few_accounts() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let mut parsed = parse_legacy_message(&launch_bytes(985, 15)).unwrap();
        parsed.instructions[1].accounts.truncate(2);
        let bytes = serialize_legacy_message(&parsed);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "only the trusted watcher can request this sign")]
    fn request_launch_rejects_non_watcher() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let bytes = launch_bytes(985, 15);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_launch(id, Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "only the trusted watcher can request this sign")]
    fn request_sweep_rejects_non_watcher() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 10, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    #[should_panic(expected = "refund must be the full refundable amount")]
    fn request_refund_rejects_partial() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 40);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 20);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes));
    }

    #[test]
    fn request_refund_accepts_full_refundable() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_trust(&mut c, 1_000, 9_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 50, SIG_A.into());
        seed_deposit(&mut c, id);
        assert_eq!(c.refundable(id, DEMO_PUBKEY.into()), 40);
        let bytes = relayer_paid_bytes([8u8; 32], deposit_bytes(), owner_bytes(), 40);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_refund(id, DEMO_PUBKEY.into(), Base64VecU8(bytes.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&bytes), SIG_D.into());
        assert_eq!(
            c.get_contribution(id, DEMO_PUBKEY.into())
                .unwrap()
                .refunded_lamports,
            40
        );
    }

    #[test]
    #[should_panic(expected = "refund must be the full refundable amount")]
    fn request_raise_refund_rejects_partial() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable_short(&mut c, 10, 2_000);
        testing_env!(context(accounts(2), PAID).build());
        c.report_deposit(id, DEMO_PUBKEY.into(), 10, SIG_A.into());
        seed_raise(&mut c, id);
        seed_deposit(&mut c, id);
        let sweep = relayer_paid_bytes([8u8; 32], deposit_bytes(), raise_bytes(), 10);
        testing_env!(context(accounts(2), PAID).build());
        let _ = c.request_sweep(id, DEMO_PUBKEY.into(), Base64VecU8(sweep.clone()));
        testing_env!(context(accounts(2), PAID).build());
        c.confirm_transfer(message_hash(&sweep), SIG_B.into());
        testing_env!(context_at(accounts(2), NearToken::from_yoctonear(0), 3_000).build());
        assert_eq!(c.try_timeout(id), TrustState::Refunding);
        assert_eq!(c.raise_refundable(id, DEMO_PUBKEY.into()), 10);
        let refund = relayer_paid_bytes([8u8; 32], raise_bytes(), owner_bytes(), 5);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_raise_refund(id, DEMO_PUBKEY.into(), Base64VecU8(refund));
    }

    #[test]
    #[should_panic(expected = "only the trusted watcher can request this sign")]
    fn request_fill_fee_rejects_non_watcher() {
        testing_env!(context(accounts(1), NearToken::from_near(1)).build());
        let mut c = Contract::new(None, None);
        let id = open_fillable(&mut c, 1_000, 9_000);
        go_executing_swept(&mut c, id, 1_000, SIG_A, SIG_B);
        let fee_tx = relayer_paid_bytes([8u8; 32], raise_bytes(), fee_bytes(), 15);
        testing_env!(context(accounts(1), PAID).build());
        let _ = c.request_fill_fee(id, Base64VecU8(fee_tx));
    }
}
