//! Shape D: pro-rata SPL TransferChecked from the raise vault's token ATA.
//!
//! Launch (Shape C) initializes the base mint with
//! `initialize_virtual_pool_with_spl_token` and pins classic Tokenkeg.
//! Token-2022 is rejected there, so distribution uses Tokenkeg only:
//! ATA seeds `[owner, Tokenkeg, mint]`, TransferChecked decimals = 6.
//!
//! A message is legacy, ≤ 1232 bytes, fee payer = relayer (account 0).
//! The raise vault is the only other signer (one MPC sign). Optional
//! `CreateIdempotent` for the destination ATA is paid by that relayer,
//! never by raise / mint / fee. Then exactly one TransferChecked of the
//! contributor's remaining share. Up to `MAX_DISTRIBUTE_BATCH` legs.
//! No other instructions. Instruction data lengths are exact.

use crate::pda::find_program_address;
use crate::shapes::dbc::{ATA_PROGRAM, TOKEN_PROGRAM};
use crate::shapes::transfer::SYSTEM_PROGRAM;
use crate::solana::error::ShapeError;
use crate::solana::message::LegacyMessage;

/// Pinned demo curve and Shape C init: classic SPL, 6 decimals.
pub const TOKEN_DECIMALS: u8 = 6;
/// One raise MPC signature. Sized so a batch still fits in 1232 bytes
/// (shared keys + 2 keys and ~40 bytes of instructions per leg).
pub const MAX_DISTRIBUTE_BATCH: usize = 5;
pub const MAX_MESSAGE_LEN: usize = 1232;

const TOKEN_TRANSFER_CHECKED: u8 = 12;
const ATA_CREATE_IDEMPOTENT: u8 = 1;
const TRANSFER_CHECKED_LEN: usize = 10;
const CREATE_LEN: usize = 1;
const TRANSFER_ACCOUNTS: usize = 4;
const CREATE_ACCOUNTS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLeg {
    pub owner: [u8; 32],
    pub dest: [u8; 32],
    pub amount: u64,
    /// `false` when TransferChecked had no preceding CreateIdempotent.
    /// `owner` is then the destination ATA until `resolve_owners`.
    pub owner_resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributeCheck {
    pub raise: [u8; 32],
    pub mint: [u8; 32],
    pub fee: [u8; 32],
    pub expected: Vec<([u8; 32], u64)>,
}

pub fn derive_ata(owner: &[u8; 32], mint: &[u8; 32], token_program: &[u8; 32]) -> [u8; 32] {
    find_program_address(
        &[owner.as_slice(), token_program.as_slice(), mint.as_slice()],
        &ATA_PROGRAM,
    )
}

fn acc<'a>(
    msg: &'a LegacyMessage,
    ix_accounts: &[u8],
    i: usize,
) -> Result<&'a [u8; 32], ShapeError> {
    let idx = *ix_accounts.get(i).ok_or(ShapeError::WrongAccountCount)? as usize;
    msg.account_keys.get(idx).ok_or(ShapeError::MissingAccounts)
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

fn is_vault(key: &[u8; 32], raise: &[u8; 32], mint: &[u8; 32], fee: &[u8; 32]) -> bool {
    key == raise || key == mint || key == fee
}

/// Structural parse. Amounts are checked against `expected` in
/// `validate_distribute`.
pub fn parse_distribute(
    msg: &LegacyMessage,
    raw_len: usize,
    raise: &[u8; 32],
    mint: &[u8; 32],
    fee: &[u8; 32],
) -> Result<Vec<ParsedLeg>, ShapeError> {
    if raw_len > MAX_MESSAGE_LEN {
        return Err(ShapeError::MessageTooLarge);
    }
    if msg.instructions.is_empty() {
        return Err(ShapeError::NoInstructions);
    }
    if msg.account_keys.is_empty() {
        return Err(ShapeError::MissingAccounts);
    }
    if msg.header.num_required_signatures != 2 {
        return Err(ShapeError::FeePayerNotRelayer);
    }
    let fee_payer = msg.account_keys[0];
    if !msg.is_signer(0) || !msg.is_writable(0) {
        return Err(ShapeError::FeePayerNotRelayer);
    }
    if is_vault(&fee_payer, raise, mint, fee) {
        return Err(ShapeError::FeePayerNotRelayer);
    }
    let raise_idx = msg
        .account_keys
        .iter()
        .position(|k| k == raise)
        .ok_or(ShapeError::SourceNotSigner)?;
    if !msg.is_signer(raise_idx) {
        return Err(ShapeError::SourceNotSigner);
    }
    for (i, key) in msg.account_keys.iter().enumerate() {
        if msg.is_signer(i) && key != &fee_payer && key != raise {
            return Err(ShapeError::FeePayerNotRelayer);
        }
    }
    if msg.is_signer(
        msg.account_keys
            .iter()
            .position(|k| k == mint)
            .unwrap_or(usize::MAX),
    ) || msg.is_signer(
        msg.account_keys
            .iter()
            .position(|k| k == fee)
            .unwrap_or(usize::MAX),
    ) {
        return Err(ShapeError::FeePayerNotRelayer);
    }

    let source = derive_ata(raise, mint, &TOKEN_PROGRAM);
    let mut legs = Vec::new();
    let mut i = 0;
    while i < msg.instructions.len() {
        let ix = &msg.instructions[i];
        let program = msg.program_id(ix).ok_or(ShapeError::MissingAccounts)?;
        if program == &ATA_PROGRAM {
            let next = msg
                .instructions
                .get(i + 1)
                .ok_or(ShapeError::ExtraInstruction)?;
            let leg = parse_create_and_transfer(msg, ix, next, &fee_payer, raise, mint, &source)?;
            legs.push(leg);
            i += 2;
        } else if program == &TOKEN_PROGRAM {
            legs.push(parse_transfer(msg, ix, raise, mint, &source, None)?);
            i += 1;
        } else if ix.data.first() == Some(&TOKEN_TRANSFER_CHECKED) {
            return Err(ShapeError::WrongTokenProgram);
        } else {
            return Err(ShapeError::ExtraInstruction);
        }
        if legs.len() > MAX_DISTRIBUTE_BATCH {
            return Err(ShapeError::TooManyTransfers);
        }
    }
    if legs.is_empty() {
        return Err(ShapeError::NoInstructions);
    }
    let mut seen = Vec::new();
    for leg in &legs {
        if seen.contains(&leg.owner) {
            return Err(ShapeError::DuplicateContributor);
        }
        seen.push(leg.owner);
    }
    Ok(legs)
}

pub fn validate_distribute(
    msg: &LegacyMessage,
    raw_len: usize,
    check: &DistributeCheck,
) -> Result<Vec<ParsedLeg>, ShapeError> {
    let legs = parse_distribute(msg, raw_len, &check.raise, &check.mint, &check.fee)?;
    resolve_owners(legs, &check.mint, &check.expected)
}

pub fn resolve_owners(
    mut legs: Vec<ParsedLeg>,
    mint: &[u8; 32],
    expected: &[([u8; 32], u64)],
) -> Result<Vec<ParsedLeg>, ShapeError> {
    for leg in &mut legs {
        if !leg.owner_resolved {
            let found = expected
                .iter()
                .find(|(owner, _)| derive_ata(owner, mint, &TOKEN_PROGRAM) == leg.dest);
            match found {
                Some((owner, _)) => {
                    leg.owner = *owner;
                    leg.owner_resolved = true;
                }
                None => return Err(ShapeError::WrongOwner),
            }
        }
        match expected.iter().find(|(owner, _)| owner == &leg.owner) {
            Some((_, amount)) if *amount == leg.amount => {}
            Some(_) => return Err(ShapeError::WrongAmount),
            None => return Err(ShapeError::WrongOwner),
        }
    }
    Ok(legs)
}

fn parse_create_and_transfer(
    msg: &LegacyMessage,
    create: &crate::solana::message::CompiledInstruction,
    transfer: &crate::solana::message::CompiledInstruction,
    fee_payer: &[u8; 32],
    raise: &[u8; 32],
    mint: &[u8; 32],
    source: &[u8; 32],
) -> Result<ParsedLeg, ShapeError> {
    if create.accounts.len() != CREATE_ACCOUNTS {
        return Err(ShapeError::WrongAccountCount);
    }
    if create.data.len() != CREATE_LEN || create.data[0] != ATA_CREATE_IDEMPOTENT {
        return Err(ShapeError::ExactData);
    }
    let payer = *acc(msg, &create.accounts, 0)?;
    if payer != *fee_payer {
        return Err(ShapeError::AtaPayerNotRelayer);
    }
    if is_vault(&payer, raise, mint, &msg.account_keys[0]) && payer != *fee_payer {
        return Err(ShapeError::AtaPayerNotRelayer);
    }
    // Raise, mint, and fee vault never pay ATA rent. `fee` is checked by
    // the caller via is_vault against the pinned fee key below.
    let owner = *acc(msg, &create.accounts, 2)?;
    if payer == *raise || payer == *mint {
        return Err(ShapeError::AtaPayerNotRelayer);
    }
    let dest = *acc(msg, &create.accounts, 1)?;
    require_acc(msg, &create.accounts, 3, mint)?;
    require_acc(msg, &create.accounts, 4, &SYSTEM_PROGRAM)?;
    require_acc(msg, &create.accounts, 5, &TOKEN_PROGRAM)?;
    let derived = derive_ata(&owner, mint, &TOKEN_PROGRAM);
    if dest != derived {
        return Err(ShapeError::WrongDestination);
    }
    let leg = parse_transfer(msg, transfer, raise, mint, source, Some(owner))?;
    if derive_ata(&leg.owner, mint, &TOKEN_PROGRAM) != dest {
        return Err(ShapeError::WrongDestination);
    }
    Ok(leg)
}

fn parse_transfer(
    msg: &LegacyMessage,
    ix: &crate::solana::message::CompiledInstruction,
    raise: &[u8; 32],
    mint: &[u8; 32],
    source: &[u8; 32],
    owner_hint: Option<[u8; 32]>,
) -> Result<ParsedLeg, ShapeError> {
    let program = msg.program_id(ix).ok_or(ShapeError::MissingAccounts)?;
    if program != &TOKEN_PROGRAM {
        return Err(ShapeError::WrongTokenProgram);
    }
    if ix.accounts.len() != TRANSFER_ACCOUNTS {
        return Err(ShapeError::WrongAccountCount);
    }
    if ix.data.len() != TRANSFER_CHECKED_LEN {
        return Err(ShapeError::ExactData);
    }
    if ix.data[0] != TOKEN_TRANSFER_CHECKED {
        return Err(ShapeError::NotTransferChecked);
    }
    let amount = u64::from_le_bytes(ix.data[1..9].try_into().unwrap());
    let decimals = ix.data[9];
    if decimals != TOKEN_DECIMALS {
        return Err(ShapeError::WrongDecimals);
    }
    if amount == 0 {
        return Err(ShapeError::WrongAmount);
    }
    let src = *acc(msg, &ix.accounts, 0)?;
    let ix_mint = *acc(msg, &ix.accounts, 1)?;
    let dest = *acc(msg, &ix.accounts, 2)?;
    let authority = *acc(msg, &ix.accounts, 3)?;
    if ix_mint != *mint {
        return Err(ShapeError::WrongMint);
    }
    if src != *source {
        return Err(ShapeError::WrongSource);
    }
    if authority != *raise {
        return Err(ShapeError::WrongSource);
    }
    let src_idx = ix.accounts[0] as usize;
    let dest_idx = ix.accounts[2] as usize;
    if !msg.is_writable(src_idx) || !msg.is_writable(dest_idx) {
        return Err(ShapeError::DestinationNotWritable);
    }
    if src == dest {
        return Err(ShapeError::WrongDestination);
    }
    if let Some(owner) = owner_hint {
        let derived = derive_ata(&owner, mint, &TOKEN_PROGRAM);
        if dest != derived {
            return Err(ShapeError::WrongDestination);
        }
        return Ok(ParsedLeg {
            owner,
            dest,
            amount,
            owner_resolved: true,
        });
    }
    Ok(ParsedLeg {
        owner: dest,
        dest,
        amount,
        owner_resolved: false,
    })
}

pub fn build_distribute_message(
    relayer: [u8; 32],
    raise: [u8; 32],
    mint: [u8; 32],
    legs: &[([u8; 32], u64, bool)],
    blockhash: [u8; 32],
) -> Vec<u8> {
    use crate::solana::message::{
        serialize_legacy_message, CompiledInstruction, LegacyMessage, MessageHeader,
    };

    let source = derive_ata(&raise, &mint, &TOKEN_PROGRAM);
    let any_create = legs.iter().any(|(_, _, create)| *create);
    let mut keys = vec![relayer, raise, source];
    let mut dests = Vec::new();
    for (owner, _, _) in legs {
        let dest = derive_ata(owner, &mint, &TOKEN_PROGRAM);
        dests.push(dest);
        keys.push(dest);
    }
    let mint_index = keys.len() as u8;
    keys.push(mint);
    let mut owner_indexes = Vec::new();
    for (owner, _, create) in legs {
        if *create {
            owner_indexes.push(keys.len() as u8);
            keys.push(*owner);
        } else {
            owner_indexes.push(0);
        }
    }
    let system_index = if any_create {
        let idx = keys.len() as u8;
        keys.push(SYSTEM_PROGRAM);
        idx
    } else {
        0
    };
    let token_index = keys.len() as u8;
    keys.push(TOKEN_PROGRAM);
    let ata_index = if any_create {
        let idx = keys.len() as u8;
        keys.push(ATA_PROGRAM);
        idx
    } else {
        0
    };

    let n_keys = keys.len();
    let readonly_unsigned =
        1 + legs.iter().filter(|(_, _, c)| *c).count() + usize::from(any_create) * 2 + 1;
    // signers: relayer writable + raise readonly. writable unsigned: source + dests.
    let _ = readonly_unsigned;
    let num_readonly_unsigned = (n_keys - 2 - (1 + legs.len())) as u8;

    let mut instructions = Vec::new();
    for (i, (owner, amount, create)) in legs.iter().enumerate() {
        let dest_index = (3 + i) as u8;
        if *create {
            instructions.push(CompiledInstruction {
                program_id_index: ata_index,
                accounts: vec![
                    0,
                    dest_index,
                    owner_indexes[i],
                    mint_index,
                    system_index,
                    token_index,
                ],
                data: vec![ATA_CREATE_IDEMPOTENT],
            });
            let _ = owner;
        }
        let mut data = vec![TOKEN_TRANSFER_CHECKED];
        data.extend_from_slice(&amount.to_le_bytes());
        data.push(TOKEN_DECIMALS);
        instructions.push(CompiledInstruction {
            program_id_index: token_index,
            accounts: vec![2, mint_index, dest_index, 1],
            data,
        });
    }

    serialize_legacy_message(&LegacyMessage {
        header: MessageHeader {
            num_required_signatures: 2,
            num_readonly_signed: 1,
            num_readonly_unsigned,
        },
        account_keys: keys,
        recent_blockhash: blockhash,
        instructions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solana::parse_legacy_message;

    fn key(tag: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = tag;
        k[31] = tag;
        k
    }

    fn check(
        raise: [u8; 32],
        mint: [u8; 32],
        fee: [u8; 32],
        legs: Vec<([u8; 32], u64)>,
    ) -> DistributeCheck {
        DistributeCheck {
            raise,
            mint,
            fee,
            expected: legs,
        }
    }

    fn honest(create: bool) -> (DistributeCheck, Vec<u8>) {
        let relayer = key(8);
        let raise = key(1);
        let mint = key(2);
        let fee = key(3);
        let owner = key(4);
        let amount = 60u64;
        let bytes =
            build_distribute_message(relayer, raise, mint, &[(owner, amount, create)], key(9));
        (check(raise, mint, fee, vec![(owner, amount)]), bytes)
    }

    #[test]
    fn accepts_single_transfer_checked() {
        let (c, bytes) = honest(false);
        let msg = parse_legacy_message(&bytes).unwrap();
        let legs = validate_distribute(&msg, bytes.len(), &c).unwrap();
        assert_eq!(legs.len(), 1);
        assert_eq!(legs[0].amount, 60);
        assert!(bytes.len() < MAX_MESSAGE_LEN);
        assert_eq!(bytes[0] & 0x80, 0);
    }

    #[test]
    fn accepts_create_idempotent_plus_transfer() {
        let (c, bytes) = honest(true);
        let msg = parse_legacy_message(&bytes).unwrap();
        let legs = validate_distribute(&msg, bytes.len(), &c).unwrap();
        assert_eq!(legs[0].owner, c.expected[0].0);
        let create = &msg.instructions[0];
        assert_eq!(create.data, vec![1]);
        assert_eq!(create.accounts.len(), 6);
        assert_eq!(msg.instructions[1].data.len(), 10);
        assert_eq!(msg.instructions[1].data[0], 12);
        assert_eq!(msg.instructions[1].data[9], TOKEN_DECIMALS);
    }

    #[test]
    fn accepts_batch_of_two() {
        let relayer = key(8);
        let raise = key(1);
        let mint = key(2);
        let fee = key(3);
        let a = key(4);
        let b = key(5);
        let bytes = build_distribute_message(
            relayer,
            raise,
            mint,
            &[(a, 60, true), (b, 40, true)],
            key(9),
        );
        let msg = parse_legacy_message(&bytes).unwrap();
        let c = check(raise, mint, fee, vec![(a, 60), (b, 40)]);
        let legs = validate_distribute(&msg, bytes.len(), &c).unwrap();
        assert_eq!(legs.len(), 2);
        assert!(bytes.len() <= MAX_MESSAGE_LEN);
    }

    #[test]
    fn rejects_wrong_owner() {
        let (c, bytes) = honest(true);
        let msg = parse_legacy_message(&bytes).unwrap();
        let mut bad = c.clone();
        bad.expected[0].0 = key(7);
        let err = validate_distribute(&msg, bytes.len(), &bad).unwrap_err();
        assert_eq!(err, ShapeError::WrongOwner);
    }

    #[test]
    fn rejects_wrong_mint() {
        let (c, mut_bytes) = honest(false);
        let mut msg = parse_legacy_message(&mut_bytes).unwrap();
        msg.account_keys[4] = key(6);
        let err = validate_distribute(&msg, mut_bytes.len(), &c).unwrap_err();
        assert_eq!(err, ShapeError::WrongMint);
    }

    #[test]
    fn rejects_over_and_under_amount() {
        let (c, bytes) = honest(false);
        let msg = parse_legacy_message(&bytes).unwrap();
        let mut over = c.clone();
        over.expected[0].1 = 59;
        assert_eq!(
            validate_distribute(&msg, bytes.len(), &over).unwrap_err(),
            ShapeError::WrongAmount
        );
        let mut under = c.clone();
        under.expected[0].1 = 61;
        assert_eq!(
            validate_distribute(&msg, bytes.len(), &under).unwrap_err(),
            ShapeError::WrongAmount
        );
    }

    #[test]
    fn rejects_extra_instruction() {
        let (c, bytes) = honest(false);
        let mut msg = parse_legacy_message(&bytes).unwrap();
        msg.instructions.push(msg.instructions[0].clone());
        let err = validate_distribute(&msg, bytes.len(), &c).unwrap_err();
        assert!(matches!(
            err,
            ShapeError::ExtraInstruction
                | ShapeError::DuplicateContributor
                | ShapeError::WrongAccountCount
                | ShapeError::ExactData
        ));
    }

    #[test]
    fn rejects_system_transfer_appended() {
        let (c, bytes) = honest(false);
        let mut msg = parse_legacy_message(&bytes).unwrap();
        msg.account_keys.push(SYSTEM_PROGRAM);
        msg.header.num_readonly_unsigned += 1;
        msg.instructions
            .push(crate::solana::message::CompiledInstruction {
                program_id_index: (msg.account_keys.len() - 1) as u8,
                accounts: vec![0, 1],
                data: vec![2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
            });
        let err = validate_distribute(&msg, bytes.len(), &c).unwrap_err();
        assert_eq!(err, ShapeError::ExtraInstruction);
    }

    #[test]
    fn rejects_wrong_source() {
        let (c, bytes) = honest(false);
        let mut msg = parse_legacy_message(&bytes).unwrap();
        msg.instructions[0].accounts[0] = 1;
        let err = validate_distribute(&msg, bytes.len(), &c).unwrap_err();
        assert_eq!(err, ShapeError::WrongSource);
    }

    #[test]
    fn rejects_ata_payer_raise() {
        let (c, bytes) = honest(true);
        let mut msg = parse_legacy_message(&bytes).unwrap();
        msg.instructions[0].accounts[0] = 1;
        let err = validate_distribute(&msg, bytes.len(), &c).unwrap_err();
        assert_eq!(err, ShapeError::AtaPayerNotRelayer);
    }

    #[test]
    fn rejects_fee_payer_raise() {
        let relayer = key(1);
        let raise = key(1);
        let mint = key(2);
        let fee = key(3);
        let owner = key(4);
        let bytes = build_distribute_message(relayer, raise, mint, &[(owner, 60, true)], key(9));
        let msg = parse_legacy_message(&bytes).unwrap();
        let c = check(raise, mint, fee, vec![(owner, 60)]);
        assert_eq!(
            validate_distribute(&msg, bytes.len(), &c).unwrap_err(),
            ShapeError::FeePayerNotRelayer
        );
    }

    #[test]
    fn rejects_trailing_instruction_data() {
        let (c, bytes) = honest(false);
        let mut msg = parse_legacy_message(&bytes).unwrap();
        msg.instructions[0].data.push(0);
        assert_eq!(
            validate_distribute(&msg, bytes.len(), &c).unwrap_err(),
            ShapeError::ExactData
        );
    }

    #[test]
    fn rejects_bad_decimals_and_token_2022_disc() {
        let (c, bytes) = honest(false);
        let mut msg = parse_legacy_message(&bytes).unwrap();
        msg.instructions[0].data[9] = 9;
        assert_eq!(
            validate_distribute(&msg, bytes.len(), &c).unwrap_err(),
            ShapeError::WrongDecimals
        );
        msg.instructions[0].data[9] = 6;
        msg.instructions[0].data[0] = 3;
        assert_eq!(
            validate_distribute(&msg, bytes.len(), &c).unwrap_err(),
            ShapeError::NotTransferChecked
        );
    }

    #[test]
    fn classic_spl_not_token_2022() {
        let token_2022 =
            crate::bs58::decode_pubkey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
        assert_ne!(token_2022, TOKEN_PROGRAM);
        let (c, bytes) = honest(false);
        let mut msg = parse_legacy_message(&bytes).unwrap();
        let token_idx = msg.instructions[0].program_id_index as usize;
        msg.account_keys[token_idx] = token_2022;
        assert_eq!(
            validate_distribute(&msg, bytes.len(), &c).unwrap_err(),
            ShapeError::WrongTokenProgram
        );
    }

    #[test]
    fn accepts_sdk_golden_distribute_message() {
        let raw = include_str!("../../tests/fixtures/sdk_distribute.json");
        let v: near_sdk::serde_json::Value = near_sdk::serde_json::from_str(raw).unwrap();
        let bytes = near_sdk::base64::engine::Engine::decode(
            &near_sdk::base64::engine::general_purpose::STANDARD,
            v["messageBase64"].as_str().unwrap(),
        )
        .unwrap();
        let msg = parse_legacy_message(&bytes).expect("golden message must parse");
        let raise = crate::bs58::decode_pubkey(v["raise"].as_str().unwrap()).unwrap();
        let mint = crate::bs58::decode_pubkey(v["mint"].as_str().unwrap()).unwrap();
        let fee = crate::bs58::decode_pubkey(v["fee"].as_str().unwrap()).unwrap();
        let mut expected = Vec::new();
        for leg in v["legs"].as_array().unwrap() {
            let owner = crate::bs58::decode_pubkey(leg["owner"].as_str().unwrap()).unwrap();
            let amount: u64 = leg["amount"].as_str().unwrap().parse().unwrap();
            expected.push((owner, amount));
        }
        let got = validate_distribute(
            &msg,
            bytes.len(),
            &DistributeCheck {
                raise,
                mint,
                fee,
                expected,
            },
        )
        .expect("honest spl-token distribute tx");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].amount, 33);
        assert_eq!(got[1].amount, 66);
        assert!(got.iter().all(|leg| leg.owner_resolved));
        assert_eq!(bytes[0] & 0x80, 0);
        assert!(bytes.len() <= MAX_MESSAGE_LEN);
        let creates = msg
            .instructions
            .iter()
            .filter(|ix| msg.program_id(ix) == Some(&ATA_PROGRAM))
            .count();
        let transfers = msg
            .instructions
            .iter()
            .filter(|ix| msg.program_id(ix) == Some(&TOKEN_PROGRAM))
            .count();
        assert_eq!(creates, 2);
        assert_eq!(transfers, 2);
        for ix in &msg.instructions {
            let program = msg.program_id(ix).unwrap();
            if program == &ATA_PROGRAM {
                assert_eq!(ix.data, vec![1]);
                assert_eq!(ix.accounts.len(), 6);
            } else {
                assert_eq!(ix.data.len(), 10);
                assert_eq!(ix.data[0], 12);
                assert_eq!(ix.data[9], TOKEN_DECIMALS);
            }
        }
    }
}
