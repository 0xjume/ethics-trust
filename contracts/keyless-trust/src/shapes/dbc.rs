//! Shape C: DBC initialize_virtual_pool_with_spl_token + first-buy swap.
//!
//! Every account on initialize, swap, ATA-create, SyncNative and the
//! allowed System transfers is pinned (derived PDAs / ATAs compared by
//! index). Relayer must pay init + ATA rent (never raise / mint / fee).
//! Raise vault may wrap buy lamports into its WSOL ATA and optionally
//! close that ATA back to itself. Swap is the IDL 15-account list
//! (referral `None` = DBC program id at index 12) plus optional
//! `Sysvar1nstructions`. ExactIn only; `minimum_amount_out` must equal
//! `LaunchCheck.min_out` (the contract's first-buy quote).

use crate::bs58;
use crate::pda::{find_program_address, ordered_keys};
use crate::shapes::transfer::{COMPUTE_BUDGET_PROGRAM, SYSTEM_PROGRAM};
use crate::solana::error::ShapeError;
use crate::solana::message::LegacyMessage;

/// `dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN`
pub const DBC_PROGRAM: [u8; 32] = [
    9, 96, 12, 165, 36, 247, 177, 183, 214, 204, 177, 195, 151, 58, 160, 51, 13, 25, 3, 218, 96,
    28, 201, 181, 222, 227, 198, 98, 180, 202, 209, 73,
];

/// `FhVo3mqL8PW5pH5U2CN4XE33DokiyZnUwuGpH2hmHLuM`
pub const POOL_AUTHORITY: [u8; 32] = [
    218, 99, 104, 31, 114, 134, 188, 200, 6, 113, 158, 44, 43, 80, 162, 1, 87, 36, 59, 252, 150,
    104, 177, 21, 32, 188, 83, 132, 62, 220, 219, 8,
];

/// Native SOL mint.
pub const WSOL_MINT: [u8; 32] = [
    6, 155, 136, 87, 254, 171, 129, 132, 251, 104, 127, 99, 70, 24, 192, 53, 218, 196, 57, 220, 26,
    235, 59, 85, 152, 160, 240, 0, 0, 0, 0, 1,
];

/// Tokenkeg classic SPL. Token-2022 initialize is not allowed.
pub const TOKEN_PROGRAM: [u8; 32] = [
    6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172, 28, 180, 133, 237,
    95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
];

pub const ATA_PROGRAM: [u8; 32] = [
    140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131, 11, 90, 19, 153, 218,
    255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
];

/// `metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s`
pub const METADATA_PROGRAM: [u8; 32] = [
    11, 112, 101, 177, 227, 209, 124, 69, 56, 157, 82, 127, 107, 4, 195, 205, 88, 184, 108, 115,
    26, 160, 253, 181, 73, 182, 209, 188, 3, 248, 41, 70,
];

/// `Sysvar1nstructions1111111111111111111111111` — optional swap remaining
/// account when the DBC config has `enableFirstSwapWithMinFee`.
pub const SYSVAR_INSTRUCTIONS: [u8; 32] = [
    6, 167, 213, 23, 24, 123, 209, 102, 53, 218, 212, 4, 85, 253, 194, 192, 193, 36, 198, 143, 33,
    86, 117, 165, 219, 186, 203, 95, 8, 0, 0, 0,
];

/// sha256("global:initialize_virtual_pool_with_spl_token")[0..8]
pub const INIT_POOL_DISC: [u8; 8] = [140, 85, 215, 176, 102, 54, 104, 79];
/// sha256("global:swap")[0..8]
pub const SWAP_DISC: [u8; 8] = [248, 198, 158, 145, 225, 117, 135, 200];
/// sha256("global:swap2")[0..8]
pub const SWAP2_DISC: [u8; 8] = [65, 75, 63, 76, 235, 91, 91, 136];

const TRANSFER_DISC: u32 = 2;
const TOKEN_SYNC_NATIVE: u8 = 17;
const TOKEN_CLOSE_ACCOUNT: u8 = 9;
const ATA_CREATE_IDEMPOTENT: u8 = 1;

const INIT_ACCOUNTS: usize = 16;
/// IDL `swap` / `swap2`: 15 declared accounts. Index 12 is optional
/// `referral_token_account` — Anchor encodes `None` as the DBC program id.
/// A 16th remaining account may be `Sysvar1nstructions`.
const SWAP_ACCOUNTS: usize = 15;
const ATA_ACCOUNTS: usize = 6;
const CLOSE_ACCOUNTS: usize = 3;
const SWAP_IX_LEN: usize = 24;
const SWAP2_IX_LEN: usize = 25;
const SWAP_MODE_EXACT_IN: u8 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCheck {
    pub dbc_program: [u8; 32],
    pub config: [u8; 32],
    pub raise: [u8; 32],
    pub fee: [u8; 32],
    pub mint: [u8; 32],
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub buy_lamports: u64,
    pub fill_fee_lamports: u64,
    pub fill_fee_already_paid: bool,
    pub min_out: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedLaunch {
    pub pool: [u8; 32],
    pub mint: [u8; 32],
    pub buy_lamports: u64,
    pub fill_fee_in_tx: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedLaunch {
    pub pool: [u8; 32],
    pub base_vault: [u8; 32],
    pub quote_vault: [u8; 32],
    pub metadata: [u8; 32],
    pub event_authority: [u8; 32],
    pub wsol_ata: [u8; 32],
    pub token_ata: [u8; 32],
}

pub fn decode_dbc_program(s: &str) -> Result<[u8; 32], ShapeError> {
    bs58::decode_pubkey(s).map_err(|_| ShapeError::UnexpectedProgram)
}

pub fn derive_dbc_event_authority() -> [u8; 32] {
    find_program_address(&[b"__event_authority"], &DBC_PROGRAM)
}

pub fn derive_dbc_pool(config: &[u8; 32], base_mint: &[u8; 32]) -> [u8; 32] {
    let (first, second) = ordered_keys(&WSOL_MINT, base_mint);
    find_program_address(
        &[
            b"pool",
            config.as_slice(),
            first.as_slice(),
            second.as_slice(),
        ],
        &DBC_PROGRAM,
    )
}

pub fn derive_dbc_token_vault(pool: &[u8; 32], mint: &[u8; 32]) -> [u8; 32] {
    find_program_address(
        &[b"token_vault", mint.as_slice(), pool.as_slice()],
        &DBC_PROGRAM,
    )
}

pub fn derive_metadata(mint: &[u8; 32]) -> [u8; 32] {
    find_program_address(
        &[b"metadata", METADATA_PROGRAM.as_slice(), mint.as_slice()],
        &METADATA_PROGRAM,
    )
}

pub fn derive_token_ata(owner: &[u8; 32], mint: &[u8; 32]) -> [u8; 32] {
    find_program_address(
        &[owner.as_slice(), TOKEN_PROGRAM.as_slice(), mint.as_slice()],
        &ATA_PROGRAM,
    )
}

pub fn derive_launch(config: &[u8; 32], mint: &[u8; 32], raise: &[u8; 32]) -> DerivedLaunch {
    let pool = derive_dbc_pool(config, mint);
    DerivedLaunch {
        base_vault: derive_dbc_token_vault(&pool, mint),
        quote_vault: derive_dbc_token_vault(&pool, &WSOL_MINT),
        metadata: derive_metadata(mint),
        event_authority: derive_dbc_event_authority(),
        wsol_ata: derive_token_ata(raise, &WSOL_MINT),
        token_ata: derive_token_ata(raise, mint),
        pool,
    }
}

fn acc<'a>(
    msg: &'a LegacyMessage,
    ix_accounts: &[u8],
    i: usize,
) -> Result<&'a [u8; 32], ShapeError> {
    let idx = *ix_accounts.get(i).ok_or(ShapeError::MissingAccounts)? as usize;
    msg.account_keys.get(idx).ok_or(ShapeError::MissingAccounts)
}

fn acc_idx(msg: &LegacyMessage, ix_accounts: &[u8], i: usize) -> Result<usize, ShapeError> {
    let idx = *ix_accounts.get(i).ok_or(ShapeError::MissingAccounts)? as usize;
    if idx >= msg.account_keys.len() {
        return Err(ShapeError::MissingAccounts);
    }
    Ok(idx)
}

fn require_acc(
    msg: &LegacyMessage,
    ix_accounts: &[u8],
    i: usize,
    expected: &[u8; 32],
) -> Result<(), ShapeError> {
    if acc(msg, ix_accounts, i)? != expected {
        return Err(ShapeError::WrongAccount);
    }
    Ok(())
}

fn read_borsh_string(data: &[u8], offset: &mut usize) -> Result<String, ShapeError> {
    if *offset + 4 > data.len() {
        return Err(ShapeError::WrongParams);
    }
    let len = u32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap()) as usize;
    *offset += 4;
    if *offset + len > data.len() {
        return Err(ShapeError::WrongParams);
    }
    let s =
        std::str::from_utf8(&data[*offset..*offset + len]).map_err(|_| ShapeError::WrongParams)?;
    *offset += len;
    Ok(s.to_string())
}

fn is_compute_budget(msg: &LegacyMessage, ix_index: usize) -> bool {
    msg.instructions
        .get(ix_index)
        .and_then(|ix| msg.program_id(ix))
        .map(|pid| pid == &COMPUTE_BUDGET_PROGRAM)
        .unwrap_or(false)
}

fn parse_init(data: &[u8]) -> Result<(String, String, String), ShapeError> {
    if data.len() < 8 || data[0..8] != INIT_POOL_DISC {
        return Err(ShapeError::MissingLaunchIx);
    }
    let mut off = 8;
    let name = read_borsh_string(data, &mut off)?;
    let symbol = read_borsh_string(data, &mut off)?;
    let uri = read_borsh_string(data, &mut off)?;
    Ok((name, symbol, uri))
}

/// `swap` = disc + `{amount_in: u64, minimum_amount_out: u64}` (exactly 24).
/// `swap2` = disc + `{amount_0, amount_1, swap_mode: u8}` (exactly 25).
/// Builder uses ExactIn (`swap_mode = 0`) with `amount_in = credited − fee`
/// and `minimum_amount_out` exactly equal to `LaunchCheck.min_out` (the
/// contract's first-buy quote). Extra trailing bytes are rejected.
fn parse_swap_amounts(data: &[u8]) -> Result<(u64, u64), ShapeError> {
    if data.len() < 8 {
        return Err(ShapeError::WrongBuyAmount);
    }
    let disc = &data[0..8];
    if disc == SWAP_DISC {
        if data.len() != SWAP_IX_LEN {
            return Err(ShapeError::WrongParams);
        }
    } else if disc == SWAP2_DISC {
        if data.len() != SWAP2_IX_LEN {
            return Err(ShapeError::WrongParams);
        }
        if data[24] != SWAP_MODE_EXACT_IN {
            return Err(ShapeError::WrongBuyAmount);
        }
    } else {
        return Err(ShapeError::MissingLaunchIx);
    }
    let amount_in = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let min_out = u64::from_le_bytes(data[16..24].try_into().unwrap());
    Ok((amount_in, min_out))
}

fn require_relayer_payer(
    payer: &[u8; 32],
    relayer: &[u8; 32],
    check: &LaunchCheck,
) -> Result<(), ShapeError> {
    if payer != relayer {
        return Err(ShapeError::WrongSource);
    }
    if relayer == &check.raise || relayer == &check.mint || relayer == &check.fee {
        return Err(ShapeError::WrongSource);
    }
    Ok(())
}

fn fee_payer(msg: &LegacyMessage) -> Result<[u8; 32], ShapeError> {
    msg.account_keys
        .first()
        .copied()
        .ok_or(ShapeError::MissingAccounts)
}

/// Validate a legacy Shape C message. Versioned / ALT messages never reach
/// here — `parse_legacy_message` rejects the 0x80 prefix first.
pub fn validate_launch(
    msg: &LegacyMessage,
    check: &LaunchCheck,
) -> Result<ValidatedLaunch, ShapeError> {
    if msg.instructions.is_empty() {
        return Err(ShapeError::NoInstructions);
    }
    if check.dbc_program != DBC_PROGRAM {
        return Err(ShapeError::UnexpectedProgram);
    }

    let derived = derive_launch(&check.config, &check.mint, &check.raise);
    let relayer = fee_payer(msg)?;

    let mut saw_init = false;
    let mut saw_swap = false;
    let mut fill_fee_in_tx = 0u64;
    let mut wrap_in_tx = 0u64;
    let mut saw_sync = false;
    let mut sync_after_wrap = false;
    let mut buy_lamports = 0u64;

    for (i, ix) in msg.instructions.iter().enumerate() {
        if is_compute_budget(msg, i) {
            continue;
        }
        let program = msg.program_id(ix).ok_or(ShapeError::MissingAccounts)?;

        if program == &SYSTEM_PROGRAM {
            if ix.accounts.len() != 2 {
                return Err(ShapeError::MissingAccounts);
            }
            if ix.data.len() < 12 {
                return Err(ShapeError::NotSystemTransfer);
            }
            let disc = u32::from_le_bytes(ix.data[0..4].try_into().unwrap());
            if disc != TRANSFER_DISC {
                return Err(ShapeError::ExtraInstruction);
            }
            let lamports = u64::from_le_bytes(ix.data[4..12].try_into().unwrap());
            let from = *acc(msg, &ix.accounts, 0)?;
            let to = *acc(msg, &ix.accounts, 1)?;
            let from_idx = acc_idx(msg, &ix.accounts, 0)?;
            if !msg.is_signer(from_idx) {
                return Err(ShapeError::SourceNotSigner);
            }
            if from != check.raise {
                return Err(ShapeError::WrongSource);
            }
            if to == check.fee {
                if lamports != check.fill_fee_lamports || check.fill_fee_lamports == 0 {
                    return Err(ShapeError::WrongFillFee);
                }
                if fill_fee_in_tx != 0 {
                    return Err(ShapeError::WrongFillFee);
                }
                fill_fee_in_tx = lamports;
            } else if to == derived.wsol_ata {
                if lamports != check.buy_lamports {
                    return Err(ShapeError::WrongWrap);
                }
                if wrap_in_tx != 0 {
                    return Err(ShapeError::WrongWrap);
                }
                wrap_in_tx = lamports;
            } else {
                return Err(ShapeError::WrongDestination);
            }
            continue;
        }

        if program == &ATA_PROGRAM {
            if ix.data.first() != Some(&ATA_CREATE_IDEMPOTENT) {
                return Err(ShapeError::ExtraInstruction);
            }
            if ix.accounts.len() != ATA_ACCOUNTS {
                return Err(ShapeError::MissingAccounts);
            }
            let payer = *acc(msg, &ix.accounts, 0)?;
            let ata = *acc(msg, &ix.accounts, 1)?;
            let owner = *acc(msg, &ix.accounts, 2)?;
            let mint = *acc(msg, &ix.accounts, 3)?;
            require_acc(msg, &ix.accounts, 4, &SYSTEM_PROGRAM)?;
            require_acc(msg, &ix.accounts, 5, &TOKEN_PROGRAM)?;
            if owner != check.raise {
                return Err(ShapeError::WrongSource);
            }
            require_relayer_payer(&payer, &relayer, check)?;
            if mint == WSOL_MINT {
                if ata != derived.wsol_ata {
                    return Err(ShapeError::WrongAccount);
                }
            } else if mint == check.mint {
                if ata != derived.token_ata {
                    return Err(ShapeError::WrongAccount);
                }
            } else {
                return Err(ShapeError::WrongMint);
            }
            continue;
        }

        if program == &TOKEN_PROGRAM {
            let disc = ix
                .data
                .first()
                .copied()
                .ok_or(ShapeError::ExtraInstruction)?;
            if disc == TOKEN_SYNC_NATIVE {
                if ix.accounts.len() != 1 {
                    return Err(ShapeError::MissingAccounts);
                }
                if acc(msg, &ix.accounts, 0)? != &derived.wsol_ata {
                    return Err(ShapeError::WrongAccount);
                }
                saw_sync = true;
                if wrap_in_tx > 0 {
                    sync_after_wrap = true;
                }
                continue;
            }
            if disc == TOKEN_CLOSE_ACCOUNT {
                if ix.accounts.len() != CLOSE_ACCOUNTS {
                    return Err(ShapeError::MissingAccounts);
                }
                require_acc(msg, &ix.accounts, 0, &derived.wsol_ata)?;
                require_acc(msg, &ix.accounts, 1, &check.raise)?;
                require_acc(msg, &ix.accounts, 2, &check.raise)?;
                let auth_idx = acc_idx(msg, &ix.accounts, 2)?;
                if !msg.is_signer(auth_idx) {
                    return Err(ShapeError::SourceNotSigner);
                }
                continue;
            }
            return Err(ShapeError::ExtraInstruction);
        }

        if program != &check.dbc_program {
            return Err(ShapeError::UnexpectedProgram);
        }
        if ix.data.len() < 8 {
            return Err(ShapeError::MissingLaunchIx);
        }
        let disc = &ix.data[0..8];
        if disc == INIT_POOL_DISC {
            if saw_init {
                return Err(ShapeError::ExtraInstruction);
            }
            saw_init = true;
            if ix.accounts.len() != INIT_ACCOUNTS {
                return Err(ShapeError::MissingAccounts);
            }
            require_acc(msg, &ix.accounts, 0, &check.config)?;
            require_acc(msg, &ix.accounts, 1, &POOL_AUTHORITY)?;
            require_acc(msg, &ix.accounts, 2, &check.raise)?;
            require_acc(msg, &ix.accounts, 3, &check.mint)?;
            require_acc(msg, &ix.accounts, 4, &WSOL_MINT)?;
            require_acc(msg, &ix.accounts, 5, &derived.pool)?;
            require_acc(msg, &ix.accounts, 6, &derived.base_vault)?;
            require_acc(msg, &ix.accounts, 7, &derived.quote_vault)?;
            require_acc(msg, &ix.accounts, 8, &derived.metadata)?;
            require_acc(msg, &ix.accounts, 9, &METADATA_PROGRAM)?;
            let payer = *acc(msg, &ix.accounts, 10)?;
            require_relayer_payer(&payer, &relayer, check)?;
            require_acc(msg, &ix.accounts, 11, &TOKEN_PROGRAM)?;
            require_acc(msg, &ix.accounts, 12, &TOKEN_PROGRAM)?;
            require_acc(msg, &ix.accounts, 13, &SYSTEM_PROGRAM)?;
            require_acc(msg, &ix.accounts, 14, &derived.event_authority)?;
            require_acc(msg, &ix.accounts, 15, &DBC_PROGRAM)?;
            let mint_idx = acc_idx(msg, &ix.accounts, 3)?;
            let creator_idx = acc_idx(msg, &ix.accounts, 2)?;
            if !msg.is_signer(creator_idx) || !msg.is_signer(mint_idx) {
                return Err(ShapeError::SourceNotSigner);
            }
            let (name, symbol, uri) = parse_init(&ix.data)?;
            if name != check.name || symbol != check.symbol || uri != check.uri {
                return Err(ShapeError::WrongParams);
            }
        } else if disc == SWAP_DISC || disc == SWAP2_DISC {
            if saw_swap {
                return Err(ShapeError::ExtraInstruction);
            }
            saw_swap = true;
            if ix.accounts.len() < SWAP_ACCOUNTS {
                return Err(ShapeError::MissingAccounts);
            }
            require_acc(msg, &ix.accounts, 0, &POOL_AUTHORITY)?;
            require_acc(msg, &ix.accounts, 1, &check.config)?;
            require_acc(msg, &ix.accounts, 2, &derived.pool)?;
            require_acc(msg, &ix.accounts, 3, &derived.wsol_ata)?;
            require_acc(msg, &ix.accounts, 4, &derived.token_ata)?;
            require_acc(msg, &ix.accounts, 5, &derived.base_vault)?;
            require_acc(msg, &ix.accounts, 6, &derived.quote_vault)?;
            require_acc(msg, &ix.accounts, 7, &check.mint)?;
            require_acc(msg, &ix.accounts, 8, &WSOL_MINT)?;
            require_acc(msg, &ix.accounts, 9, &check.raise)?;
            require_acc(msg, &ix.accounts, 10, &TOKEN_PROGRAM)?;
            require_acc(msg, &ix.accounts, 11, &TOKEN_PROGRAM)?;
            // Anchor optional None = the program id itself. A real referral ATA is rejected.
            require_acc(msg, &ix.accounts, 12, &DBC_PROGRAM)?;
            require_acc(msg, &ix.accounts, 13, &derived.event_authority)?;
            require_acc(msg, &ix.accounts, 14, &DBC_PROGRAM)?;
            if ix.accounts.len() == SWAP_ACCOUNTS + 1 {
                require_acc(msg, &ix.accounts, 15, &SYSVAR_INSTRUCTIONS)?;
            } else if ix.accounts.len() != SWAP_ACCOUNTS {
                return Err(ShapeError::MissingAccounts);
            }
            let payer_idx = acc_idx(msg, &ix.accounts, 9)?;
            if !msg.is_signer(payer_idx) {
                return Err(ShapeError::SourceNotSigner);
            }
            let (amount_in, min_out) = parse_swap_amounts(&ix.data)?;
            if amount_in != check.buy_lamports {
                return Err(ShapeError::WrongBuyAmount);
            }
            if min_out != check.min_out {
                return Err(ShapeError::WrongMinOut);
            }
            buy_lamports = amount_in;
        } else {
            return Err(ShapeError::ExtraInstruction);
        }
    }

    if !saw_init || !saw_swap {
        return Err(ShapeError::MissingLaunchIx);
    }
    let expected_fee = if check.fill_fee_already_paid {
        0
    } else {
        check.fill_fee_lamports
    };
    if fill_fee_in_tx != expected_fee {
        return Err(ShapeError::WrongFillFee);
    }
    if wrap_in_tx != check.buy_lamports || !saw_sync || !sync_after_wrap {
        return Err(ShapeError::WrongWrap);
    }

    Ok(ValidatedLaunch {
        pool: derived.pool,
        mint: check.mint,
        buy_lamports,
        fill_fee_in_tx,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::solana::message::{
        serialize_legacy_message, CompiledInstruction, LegacyMessage, MessageHeader,
    };
    use crate::solana::parse_legacy_message;

    fn key(tag: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = tag;
        k[31] = tag;
        k
    }

    fn borsh_str(s: &str) -> Vec<u8> {
        let mut o = (s.len() as u32).to_le_bytes().to_vec();
        o.extend_from_slice(s.as_bytes());
        o
    }

    fn init_data(name: &str, symbol: &str, uri: &str) -> Vec<u8> {
        let mut d = INIT_POOL_DISC.to_vec();
        d.extend(borsh_str(name));
        d.extend(borsh_str(symbol));
        d.extend(borsh_str(uri));
        d
    }

    fn swap_data(amount_in: u64, min_out: u64) -> Vec<u8> {
        let mut d = SWAP_DISC.to_vec();
        d.extend_from_slice(&amount_in.to_le_bytes());
        d.extend_from_slice(&min_out.to_le_bytes());
        d
    }

    fn transfer_data(lamports: u64) -> Vec<u8> {
        let mut data = vec![2, 0, 0, 0];
        data.extend_from_slice(&lamports.to_le_bytes());
        data
    }

    /// Full pinned Shape C: fee, ATA WSOL, wrap, SyncNative, ATA token, init, swap, close.
    fn launch_message(
        relayer: [u8; 32],
        raise: [u8; 32],
        mint: [u8; 32],
        fee: [u8; 32],
        config: [u8; 32],
        buy: u64,
        fee_lamports: u64,
        min_out: u64,
        name: &str,
        symbol: &str,
        uri: &str,
    ) -> LegacyMessage {
        let d = derive_launch(&config, &mint, &raise);
        let keys = vec![
            relayer,
            raise,
            mint,
            fee,
            config,
            POOL_AUTHORITY,
            WSOL_MINT,
            d.pool,
            d.base_vault,
            d.quote_vault,
            d.metadata,
            METADATA_PROGRAM,
            TOKEN_PROGRAM,
            ATA_PROGRAM,
            SYSTEM_PROGRAM,
            DBC_PROGRAM,
            d.event_authority,
            d.wsol_ata,
            d.token_ata,
        ];
        // indices: 0 relayer, 1 raise, 2 mint, 3 fee, 4 config, 5 auth, 6 wsol,
        // 7 pool, 8 base_vault, 9 quote_vault, 10 meta, 11 meta_prog, 12 token,
        // 13 ata_prog, 14 system, 15 dbc, 16 event, 17 wsol_ata, 18 token_ata
        let init_accounts = vec![4, 5, 1, 2, 6, 7, 8, 9, 10, 11, 0, 12, 12, 14, 16, 15];
        let swap_accounts = vec![5, 4, 7, 17, 18, 8, 9, 2, 6, 1, 12, 12, 15, 16, 15];
        let mut instructions = Vec::new();
        if fee_lamports > 0 {
            instructions.push(CompiledInstruction {
                program_id_index: 14,
                accounts: vec![1, 3],
                data: transfer_data(fee_lamports),
            });
        }
        instructions.extend([
            CompiledInstruction {
                program_id_index: 13,
                accounts: vec![0, 17, 1, 6, 14, 12],
                data: vec![ATA_CREATE_IDEMPOTENT],
            },
            CompiledInstruction {
                program_id_index: 14,
                accounts: vec![1, 17],
                data: transfer_data(buy),
            },
            CompiledInstruction {
                program_id_index: 12,
                accounts: vec![17],
                data: vec![TOKEN_SYNC_NATIVE],
            },
            CompiledInstruction {
                program_id_index: 13,
                accounts: vec![0, 18, 1, 2, 14, 12],
                data: vec![ATA_CREATE_IDEMPOTENT],
            },
            CompiledInstruction {
                program_id_index: 15,
                accounts: init_accounts,
                data: init_data(name, symbol, uri),
            },
            CompiledInstruction {
                program_id_index: 15,
                accounts: swap_accounts,
                data: swap_data(buy, min_out),
            },
            CompiledInstruction {
                program_id_index: 12,
                accounts: vec![17, 1, 1],
                data: vec![TOKEN_CLOSE_ACCOUNT],
            },
        ]);
        LegacyMessage {
            header: MessageHeader {
                num_required_signatures: 3,
                num_readonly_signed: 0,
                num_readonly_unsigned: 8,
            },
            account_keys: keys,
            recent_blockhash: key(0xff),
            instructions,
        }
    }

    fn check(raise: [u8; 32], mint: [u8; 32], fee: [u8; 32], config: [u8; 32]) -> LaunchCheck {
        LaunchCheck {
            dbc_program: DBC_PROGRAM,
            config,
            raise,
            fee,
            mint,
            name: "Demo".into(),
            symbol: "DEMO".into(),
            uri: "https://example.com/m.json".into(),
            buy_lamports: 985,
            fill_fee_lamports: 15,
            fill_fee_already_paid: false,
            min_out: 1,
        }
    }

    fn standard() -> ([u8; 32], [u8; 32], [u8; 32], [u8; 32], LegacyMessage) {
        let raise = key(1);
        let mint = key(2);
        let fee = key(3);
        let config = key(4);
        let msg = launch_message(
            key(8),
            raise,
            mint,
            fee,
            config,
            985,
            15,
            1,
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        );
        (raise, mint, fee, config, msg)
    }

    #[test]
    fn accepts_relayer_paid_create_and_buy() {
        let (raise, mint, fee, config, msg) = standard();
        let got = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap();
        let derived = derive_launch(&config, &mint, &raise);
        assert_eq!(got.pool, derived.pool);
        assert_eq!(got.mint, mint);
        assert_eq!(got.buy_lamports, 985);
        assert_eq!(got.fill_fee_in_tx, 15);
        let bytes = serialize_legacy_message(&msg);
        assert!(
            bytes.len() < 1232,
            "launch message is {} bytes",
            bytes.len()
        );
        assert!(bytes[0] & 0x80 == 0);
    }

    #[test]
    fn rejects_tampered_config() {
        let (_raise, mint, fee, _config, msg) = standard();
        let err = validate_launch(&msg, &check(key(1), mint, fee, key(9))).unwrap_err();
        assert_eq!(err, ShapeError::WrongAccount);
    }

    #[test]
    fn rejects_tampered_mint() {
        let (raise, _mint, fee, config, msg) = standard();
        let err = validate_launch(&msg, &check(raise, key(9), fee, config)).unwrap_err();
        assert!(
            err == ShapeError::WrongMint || err == ShapeError::WrongAccount,
            "{err:?}"
        );
    }

    #[test]
    fn rejects_wrong_buy_amount() {
        let (raise, mint, fee, config, msg) = standard();
        let mut c = check(raise, mint, fee, config);
        c.buy_lamports = 500;
        let err = validate_launch(&msg, &c).unwrap_err();
        assert_eq!(err, ShapeError::WrongWrap);
    }

    #[test]
    fn rejects_wrong_name() {
        let msg = launch_message(
            key(8),
            key(1),
            key(2),
            key(3),
            key(4),
            985,
            15,
            1,
            "Nope",
            "DEMO",
            "https://example.com/m.json",
        );
        let err = validate_launch(&msg, &check(key(1), key(2), key(3), key(4))).unwrap_err();
        assert_eq!(err, ShapeError::WrongParams);
    }

    #[test]
    fn rejects_extra_instruction() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.instructions.push(CompiledInstruction {
            program_id_index: 15,
            accounts: vec![5, 4, 7, 17, 18, 8, 9, 2, 6, 1, 12, 12, 15, 16, 15],
            data: swap_data(1, 1),
        });
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::ExtraInstruction);
    }

    #[test]
    fn rejects_wrong_program() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.account_keys[15] = key(99);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::UnexpectedProgram);
    }

    #[test]
    fn rejects_versioned_alts_via_parser() {
        let (_raise, _mint, _fee, _config, msg) = standard();
        let mut bytes = serialize_legacy_message(&msg);
        bytes[0] |= 0x80;
        assert!(parse_legacy_message(&bytes).is_err());
    }

    #[test]
    fn rejects_missing_fill_fee() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.instructions.remove(0);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongFillFee);
    }

    #[test]
    fn accepts_when_fill_fee_already_paid() {
        let msg = launch_message(
            key(8),
            key(1),
            key(2),
            key(3),
            key(4),
            985,
            0,
            1,
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        );
        let mut c = check(key(1), key(2), key(3), key(4));
        c.fill_fee_already_paid = true;
        assert!(validate_launch(&msg, &c).is_ok());
    }

    #[test]
    fn rejects_extra_system_transfer_from_raise() {
        let (raise, mint, fee, config, mut msg) = standard();
        let attacker = key(99);
        msg.account_keys.push(attacker);
        let attacker_idx = (msg.account_keys.len() - 1) as u8;
        msg.instructions.insert(
            1,
            CompiledInstruction {
                program_id_index: 14,
                accounts: vec![1, attacker_idx],
                data: transfer_data(1),
            },
        );
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongDestination);
    }

    #[test]
    fn rejects_redirected_swap_output_ata() {
        let (raise, mint, fee, config, mut msg) = standard();
        // swap output is accounts[4] → token ata index 18; point it at fee vault
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.accounts[4] = 3;
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongAccount);
    }

    #[test]
    fn rejects_ata_create_with_too_few_accounts() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.instructions[1].accounts.truncate(2);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::MissingAccounts);
    }

    #[test]
    fn rejects_init_with_too_few_accounts() {
        let (raise, mint, fee, config, mut msg) = standard();
        let init = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&INIT_POOL_DISC))
            .unwrap();
        init.accounts.truncate(3);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::MissingAccounts);
    }

    #[test]
    fn rejects_swap_with_too_few_accounts() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.accounts.truncate(2);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::MissingAccounts);
    }

    #[test]
    fn rejects_swap_with_extra_referral_account() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.accounts[12] = 3; // real account instead of DBC program (None)
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongAccount);
    }

    #[test]
    fn rejects_legacy_14_account_swap_without_referral_placeholder() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.accounts.remove(12);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::MissingAccounts);
    }

    #[test]
    fn accepts_swap_with_instructions_sysvar() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.account_keys.push(SYSVAR_INSTRUCTIONS);
        let sys_idx = (msg.account_keys.len() - 1) as u8;
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.accounts.push(sys_idx);
        assert!(validate_launch(&msg, &check(raise, mint, fee, config)).is_ok());
    }

    #[test]
    fn rejects_system_transfer_from_mint() {
        let (raise, mint, fee, config, mut msg) = standard();
        let attacker = key(99);
        msg.account_keys.push(attacker);
        let attacker_idx = (msg.account_keys.len() - 1) as u8;
        msg.instructions.insert(
            1,
            CompiledInstruction {
                program_id_index: 14,
                accounts: vec![2, attacker_idx],
                data: transfer_data(1),
            },
        );
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongSource);
    }

    #[test]
    fn rejects_wrap_without_sync_native() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.instructions
            .retain(|ix| ix.data.first() != Some(&TOKEN_SYNC_NATIVE));
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongWrap);
    }

    #[test]
    fn rejects_close_account_to_attacker() {
        let (raise, mint, fee, config, mut msg) = standard();
        let close = msg.instructions.last_mut().unwrap();
        close.accounts[1] = 3; // destination = fee vault
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongAccount);
    }

    #[test]
    fn accepts_close_account_on_raise_wsol_ata() {
        let (raise, mint, fee, config, msg) = standard();
        assert!(msg.instructions.last().unwrap().data.first() == Some(&TOKEN_CLOSE_ACCOUNT));
        assert!(validate_launch(&msg, &check(raise, mint, fee, config)).is_ok());
    }

    fn swap2_data(amount_0: u64, amount_1: u64, mode: u8) -> Vec<u8> {
        let mut d = SWAP2_DISC.to_vec();
        d.extend_from_slice(&amount_0.to_le_bytes());
        d.extend_from_slice(&amount_1.to_le_bytes());
        d.push(mode);
        d
    }

    #[test]
    fn rejects_swap2_exact_out_mode() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.data = swap2_data(985, 1, 2);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongBuyAmount);
    }

    #[test]
    fn accepts_swap2_exact_in_mode() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.data = swap2_data(985, 1, SWAP_MODE_EXACT_IN);
        assert!(validate_launch(&msg, &check(raise, mint, fee, config)).is_ok());
    }

    #[test]
    fn rejects_swap_with_trailing_data() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.data.push(0);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongParams);
    }

    #[test]
    fn rejects_swap2_partial_fill_mode() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.data = swap2_data(985, 1, 1);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongBuyAmount);
    }

    #[test]
    fn rejects_swap2_with_trailing_data() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        let mut data = swap2_data(985, 1, SWAP_MODE_EXACT_IN);
        data.push(0);
        swap.data = data;
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongParams);
    }

    #[test]
    fn rejects_swap_zero_min_out() {
        let (raise, mint, fee, config, mut msg) = standard();
        let swap = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&SWAP_DISC))
            .unwrap();
        swap.data = swap_data(985, 0);
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongMinOut);
    }

    #[test]
    fn rejects_min_out_other_than_the_check() {
        let (raise, mint, fee, config, msg) = standard();
        let mut c = check(raise, mint, fee, config);
        c.min_out = 2;
        let err = validate_launch(&msg, &c).unwrap_err();
        assert_eq!(err, ShapeError::WrongMinOut);
    }

    #[test]
    fn rejects_raise_as_ata_payer() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.instructions[1].accounts[0] = 1;
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongSource);
    }

    #[test]
    fn rejects_raise_as_init_payer() {
        let (raise, mint, fee, config, mut msg) = standard();
        let init = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&INIT_POOL_DISC))
            .unwrap();
        init.accounts[10] = 1;
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongSource);
    }

    #[test]
    fn rejects_fee_as_ata_payer() {
        let (raise, mint, fee, config, mut msg) = standard();
        msg.instructions[1].accounts[0] = 3;
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongSource);
    }

    #[test]
    fn rejects_mint_as_init_payer() {
        let (raise, mint, fee, config, mut msg) = standard();
        let init = msg
            .instructions
            .iter_mut()
            .find(|ix| ix.data.starts_with(&INIT_POOL_DISC))
            .unwrap();
        init.accounts[10] = 2;
        let err = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap_err();
        assert_eq!(err, ShapeError::WrongSource);
    }

    #[test]
    fn accepts_sdk_golden_launch_message() {
        let raw = include_str!("../../tests/fixtures/sdk_launch.json");
        let v: near_sdk::serde_json::Value = near_sdk::serde_json::from_str(raw).unwrap();
        let bytes = near_sdk::base64::engine::Engine::decode(
            &near_sdk::base64::engine::general_purpose::STANDARD,
            v["messageBase64"].as_str().unwrap(),
        )
        .unwrap();
        let msg = parse_legacy_message(&bytes).expect("golden message must parse");
        let raise = bs58::decode_pubkey(v["raise"].as_str().unwrap()).unwrap();
        let mint = bs58::decode_pubkey(v["mint"].as_str().unwrap()).unwrap();
        let fee = bs58::decode_pubkey(v["fee"].as_str().unwrap()).unwrap();
        let config = bs58::decode_pubkey(v["config"].as_str().unwrap()).unwrap();
        let got = validate_launch(&msg, &check(raise, mint, fee, config)).expect("honest SDK tx");
        assert_eq!(got.buy_lamports, 985);
        assert_eq!(got.fill_fee_in_tx, 15);
        let swap = msg
            .instructions
            .iter()
            .find(|ix| ix.data.starts_with(&SWAP_DISC) || ix.data.starts_with(&SWAP2_DISC))
            .unwrap();
        assert_eq!(swap.accounts.len(), 16);
        assert_eq!(acc(&msg, &swap.accounts, 12).unwrap(), &DBC_PROGRAM);
        assert_eq!(acc(&msg, &swap.accounts, 15).unwrap(), &SYSVAR_INSTRUCTIONS);
        assert_eq!(swap.data.len(), 24);
    }

    pub(crate) fn fixture_launch_bytes(
        relayer: [u8; 32],
        raise: [u8; 32],
        mint: [u8; 32],
        fee: [u8; 32],
        config: [u8; 32],
        _pool: [u8; 32],
        buy: u64,
        fee_lamports: u64,
        min_out: u64,
        name: &str,
        symbol: &str,
        uri: &str,
    ) -> Vec<u8> {
        serialize_legacy_message(&launch_message(
            relayer,
            raise,
            mint,
            fee,
            config,
            buy,
            fee_lamports,
            min_out,
            name,
            symbol,
            uri,
        ))
    }

    pub(crate) fn fixture_derived_pool(
        config: [u8; 32],
        mint: [u8; 32],
        raise: [u8; 32],
    ) -> [u8; 32] {
        derive_launch(&config, &mint, &raise).pool
    }

    #[test]
    fn dbc_program_matches_devnet_id() {
        assert_eq!(
            bs58::decode_pubkey("dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN").unwrap(),
            DBC_PROGRAM
        );
        assert_eq!(
            bs58::decode_pubkey("FhVo3mqL8PW5pH5U2CN4XE33DokiyZnUwuGpH2hmHLuM").unwrap(),
            POOL_AUTHORITY
        );
        assert_eq!(
            bs58::decode_pubkey("Sysvar1nstructions1111111111111111111111111").unwrap(),
            SYSVAR_INSTRUCTIONS
        );
    }
}
