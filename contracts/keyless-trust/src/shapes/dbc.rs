//! Shape C: DBC initialize_virtual_pool_with_spl_token + first-buy swap.
//!
//! Optional ComputeBudget, one System transfer of the fill fee
//! (raise → fee vault), ATA create-idempotent / SyncNative helpers,
//! then exactly one initialize and one swap/swap2. No ALTs, no extra
//! programs. Token-2022 initialize is rejected.

use crate::bs58;
use crate::solana::error::ShapeError;
use crate::solana::message::LegacyMessage;
use crate::shapes::transfer::{COMPUTE_BUDGET_PROGRAM, SYSTEM_PROGRAM};

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

/// sha256("global:initialize_virtual_pool_with_spl_token")[0..8]
pub const INIT_POOL_DISC: [u8; 8] = [140, 85, 215, 176, 102, 54, 104, 79];
/// sha256("global:swap")[0..8]
pub const SWAP_DISC: [u8; 8] = [248, 198, 158, 145, 225, 117, 135, 200];
/// sha256("global:swap2")[0..8]
pub const SWAP2_DISC: [u8; 8] = [65, 75, 63, 76, 235, 91, 91, 136];

const TRANSFER_DISC: u32 = 2;
const TOKEN_SYNC_NATIVE: u8 = 17;
const ATA_CREATE_IDEMPOTENT: u8 = 1;

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

pub fn decode_dbc_program(s: &str) -> Result<[u8; 32], ShapeError> {
    bs58::decode_pubkey(s).map_err(|_| ShapeError::UnexpectedProgram)
}

fn acc<'a>(msg: &'a LegacyMessage, ix_accounts: &[u8], i: usize) -> Result<&'a [u8; 32], ShapeError> {
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

fn read_borsh_string(data: &[u8], offset: &mut usize) -> Result<String, ShapeError> {
    if *offset + 4 > data.len() {
        return Err(ShapeError::WrongParams);
    }
    let len = u32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap()) as usize;
    *offset += 4;
    if *offset + len > data.len() {
        return Err(ShapeError::WrongParams);
    }
    let s = std::str::from_utf8(&data[*offset..*offset + len]).map_err(|_| ShapeError::WrongParams)?;
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

fn parse_swap_amounts(data: &[u8]) -> Result<(u64, u64), ShapeError> {
    if data.len() < 24 {
        return Err(ShapeError::WrongBuyAmount);
    }
    let disc = &data[0..8];
    if disc != SWAP_DISC && disc != SWAP2_DISC {
        return Err(ShapeError::MissingLaunchIx);
    }
    let amount_in = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let min_out = u64::from_le_bytes(data[16..24].try_into().unwrap());
    Ok((amount_in, min_out))
}

/// Validate a legacy Shape C message. Versioned / ALT messages never reach
/// here — `parse_legacy_message` rejects the 0x80 prefix first.
pub fn validate_launch(msg: &LegacyMessage, check: &LaunchCheck) -> Result<ValidatedLaunch, ShapeError> {
    if msg.instructions.is_empty() {
        return Err(ShapeError::NoInstructions);
    }

    let mut saw_init = false;
    let mut saw_swap = false;
    let mut fill_fee_in_tx = 0u64;
    let mut pool = [0u8; 32];
    let mut mint = [0u8; 32];
    let mut buy_lamports = 0u64;

    for (i, ix) in msg.instructions.iter().enumerate() {
        if is_compute_budget(msg, i) {
            continue;
        }
        let program = msg.program_id(ix).ok_or(ShapeError::MissingAccounts)?;

        if program == &SYSTEM_PROGRAM {
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
            }
            // else: wSOL wrap into the raise ATA — allowed
            continue;
        }

        if program == &ATA_PROGRAM {
            if ix.data.first() != Some(&ATA_CREATE_IDEMPOTENT) {
                return Err(ShapeError::ExtraInstruction);
            }
            // owner is account 2; must be the raise vault
            if ix.accounts.len() >= 3 {
                let owner = *acc(msg, &ix.accounts, 2)?;
                if owner != check.raise {
                    return Err(ShapeError::WrongSource);
                }
            }
            continue;
        }

        if program == &TOKEN_PROGRAM {
            if ix.data.first() != Some(&TOKEN_SYNC_NATIVE) {
                return Err(ShapeError::ExtraInstruction);
            }
            continue;
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
            if ix.accounts.len() < 11 {
                return Err(ShapeError::MissingAccounts);
            }
            let config = *acc(msg, &ix.accounts, 0)?;
            let authority = *acc(msg, &ix.accounts, 1)?;
            let creator = *acc(msg, &ix.accounts, 2)?;
            let base_mint = *acc(msg, &ix.accounts, 3)?;
            let quote = *acc(msg, &ix.accounts, 4)?;
            pool = *acc(msg, &ix.accounts, 5)?;
            let payer = *acc(msg, &ix.accounts, 10)?;
            let mint_idx = acc_idx(msg, &ix.accounts, 3)?;
            let creator_idx = acc_idx(msg, &ix.accounts, 2)?;
            if config != check.config {
                return Err(ShapeError::WrongConfig);
            }
            if authority != POOL_AUTHORITY {
                return Err(ShapeError::UnexpectedProgram);
            }
            if creator != check.raise || payer != check.raise {
                return Err(ShapeError::WrongSource);
            }
            if !msg.is_signer(creator_idx) || !msg.is_signer(mint_idx) {
                return Err(ShapeError::SourceNotSigner);
            }
            if base_mint != check.mint {
                return Err(ShapeError::WrongMint);
            }
            if quote != WSOL_MINT {
                return Err(ShapeError::WrongConfig);
            }
            let (name, symbol, uri) = parse_init(&ix.data)?;
            if name != check.name || symbol != check.symbol || uri != check.uri {
                return Err(ShapeError::WrongParams);
            }
            mint = base_mint;
        } else if disc == SWAP_DISC || disc == SWAP2_DISC {
            if saw_swap {
                return Err(ShapeError::ExtraInstruction);
            }
            saw_swap = true;
            if ix.accounts.len() < 10 {
                return Err(ShapeError::MissingAccounts);
            }
            let config = *acc(msg, &ix.accounts, 1)?;
            let swap_pool = *acc(msg, &ix.accounts, 2)?;
            let payer = *acc(msg, &ix.accounts, 9)?;
            let payer_idx = acc_idx(msg, &ix.accounts, 9)?;
            if config != check.config {
                return Err(ShapeError::WrongConfig);
            }
            if payer != check.raise || !msg.is_signer(payer_idx) {
                return Err(ShapeError::WrongSource);
            }
            if saw_init && swap_pool != pool {
                return Err(ShapeError::WrongConfig);
            }
            let (amount_in, min_out) = parse_swap_amounts(&ix.data)?;
            if amount_in != check.buy_lamports {
                return Err(ShapeError::WrongBuyAmount);
            }
            if min_out < check.min_out {
                return Err(ShapeError::WrongBuyAmount);
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

    Ok(ValidatedLaunch {
        pool,
        mint,
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

    /// Layout: 0 relayer, 1 raise, 2 mint, 3 fee, 4 config, 5 auth, 6 wsol,
    /// 7 pool, 8–10 unused vaults/meta, 11 payer-slot unused, 12 system, 13 dbc
    fn launch_message(
        relayer: [u8; 32],
        raise: [u8; 32],
        mint: [u8; 32],
        fee: [u8; 32],
        config: [u8; 32],
        pool: [u8; 32],
        buy: u64,
        fee_lamports: u64,
        name: &str,
        symbol: &str,
        uri: &str,
    ) -> LegacyMessage {
        let keys = vec![
            relayer,
            raise,
            mint,
            fee,
            config,
            POOL_AUTHORITY,
            WSOL_MINT,
            pool,
            key(20),
            key(21),
            key(22),
            SYSTEM_PROGRAM,
            DBC_PROGRAM,
        ];
        // initialize accounts: config, auth, creator, mint, quote, pool, vaults, meta, meta_prog, payer
        // indices: 4, 5, 1, 2, 6, 7, 8, 9, 10, 10, 1   — payer is raise (1)
        // we only need first 11 for the checker
        let init_accounts = vec![4, 5, 1, 2, 6, 7, 8, 9, 10, 10, 1];
        let swap_accounts = vec![5, 4, 7, 8, 9, 8, 9, 2, 6, 1];
        LegacyMessage {
            header: MessageHeader {
                num_required_signatures: 3,
                num_readonly_signed: 0,
                num_readonly_unsigned: 4,
            },
            account_keys: keys,
            recent_blockhash: key(0xff),
            instructions: vec![
                CompiledInstruction {
                    program_id_index: 11,
                    accounts: vec![1, 3],
                    data: transfer_data(fee_lamports),
                },
                CompiledInstruction {
                    program_id_index: 12,
                    accounts: init_accounts,
                    data: init_data(name, symbol, uri),
                },
                CompiledInstruction {
                    program_id_index: 12,
                    accounts: swap_accounts,
                    data: swap_data(buy, 1),
                },
            ],
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

    #[test]
    fn accepts_relayer_paid_create_and_buy() {
        let raise = key(1);
        let mint = key(2);
        let fee = key(3);
        let config = key(4);
        let pool = key(7);
        let msg = launch_message(
            key(8),
            raise,
            mint,
            fee,
            config,
            pool,
            985,
            15,
            "Demo",
            "DEMO",
            "https://example.com/m.json",
        );
        let got = validate_launch(&msg, &check(raise, mint, fee, config)).unwrap();
        assert_eq!(got.pool, pool);
        assert_eq!(got.mint, mint);
        assert_eq!(got.buy_lamports, 985);
        assert_eq!(got.fill_fee_in_tx, 15);
        let bytes = serialize_legacy_message(&msg);
        assert!(bytes.len() < 1232);
        assert!(bytes[0] & 0x80 == 0);
    }

    #[test]
    fn rejects_tampered_config() {
        let msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        let err = validate_launch(&msg, &check(key(1), key(2), key(3), key(9))).unwrap_err();
        assert_eq!(err, ShapeError::WrongConfig);
    }

    #[test]
    fn rejects_tampered_mint() {
        let msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        let err = validate_launch(&msg, &check(key(1), key(9), key(3), key(4))).unwrap_err();
        assert_eq!(err, ShapeError::WrongMint);
    }

    #[test]
    fn rejects_wrong_buy_amount() {
        let msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 500, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        let err = validate_launch(&msg, &check(key(1), key(2), key(3), key(4))).unwrap_err();
        assert_eq!(err, ShapeError::WrongBuyAmount);
    }

    #[test]
    fn rejects_wrong_name() {
        let msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Nope", "DEMO",
            "https://example.com/m.json",
        );
        let err = validate_launch(&msg, &check(key(1), key(2), key(3), key(4))).unwrap_err();
        assert_eq!(err, ShapeError::WrongParams);
    }

    #[test]
    fn rejects_extra_instruction() {
        let mut msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        msg.instructions.push(CompiledInstruction {
            program_id_index: 12,
            accounts: vec![5, 4, 7, 8, 9, 8, 9, 2, 6, 1],
            data: swap_data(1, 1),
        });
        let err = validate_launch(&msg, &check(key(1), key(2), key(3), key(4))).unwrap_err();
        assert_eq!(err, ShapeError::ExtraInstruction);
    }

    #[test]
    fn rejects_wrong_program() {
        let mut msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        msg.account_keys[12] = key(99);
        let err = validate_launch(&msg, &check(key(1), key(2), key(3), key(4))).unwrap_err();
        assert_eq!(err, ShapeError::UnexpectedProgram);
    }

    #[test]
    fn rejects_versioned_alts_via_parser() {
        let msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        let mut bytes = serialize_legacy_message(&msg);
        bytes[0] |= 0x80;
        assert!(parse_legacy_message(&bytes).is_err());
    }

    #[test]
    fn rejects_missing_fill_fee() {
        let mut msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        msg.instructions.remove(0);
        let err = validate_launch(&msg, &check(key(1), key(2), key(3), key(4))).unwrap_err();
        assert_eq!(err, ShapeError::WrongFillFee);
    }

    #[test]
    fn accepts_when_fill_fee_already_paid() {
        let mut msg = launch_message(
            key(8), key(1), key(2), key(3), key(4), key(7), 985, 15, "Demo", "DEMO",
            "https://example.com/m.json",
        );
        msg.instructions.remove(0);
        let mut c = check(key(1), key(2), key(3), key(4));
        c.fill_fee_already_paid = true;
        assert!(validate_launch(&msg, &c).is_ok());
    }

    pub(crate) fn fixture_launch_bytes(
        relayer: [u8; 32],
        raise: [u8; 32],
        mint: [u8; 32],
        fee: [u8; 32],
        config: [u8; 32],
        pool: [u8; 32],
        buy: u64,
        fee_lamports: u64,
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
            pool,
            buy,
            fee_lamports,
            name,
            symbol,
            uri,
        ))
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
    }
}
