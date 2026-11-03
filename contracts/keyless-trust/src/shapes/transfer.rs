//! Shape A (subset): SystemProgram::Transfer to a fixed destination, amount-capped.

use crate::solana::error::ShapeError;
use crate::solana::message::LegacyMessage;

/// System program id (`11111111111111111111111111111111`).
pub const SYSTEM_PROGRAM: [u8; 32] = [0u8; 32];

/// ComputeBudget111111111111111111111111111111
pub const COMPUTE_BUDGET_PROGRAM: [u8; 32] = [
    0x03, 0x06, 0x46, 0x6f, 0xe5, 0x21, 0x17, 0x32, 0xff, 0xec, 0xad, 0xba, 0x72, 0xc3, 0x9b, 0xe7,
    0xbc, 0x8c, 0xe5, 0xbb, 0xc5, 0xf7, 0x12, 0x6b, 0x2c, 0x43, 0x9b, 0x3a, 0x40, 0x00, 0x00, 0x00,
];

const TRANSFER_DISCRIMINATOR: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferCheck {
    pub expected_destination: [u8; 32],
    pub max_lamports: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTransfer {
    pub from: [u8; 32],
    pub to: [u8; 32],
    pub lamports: u64,
}

fn is_compute_budget(msg: &LegacyMessage, ix_index: usize) -> bool {
    msg.instructions
        .get(ix_index)
        .and_then(|ix| msg.program_id(ix))
        .map(|pid| pid == &COMPUTE_BUDGET_PROGRAM)
        .unwrap_or(false)
}

/// Accept a legacy message whose non-budget instructions are exactly one
/// `SystemProgram::Transfer` from the fee payer to `expected_destination`,
/// with `lamports <= max_lamports`.
pub fn validate_system_transfer(
    msg: &LegacyMessage,
    check: &TransferCheck,
) -> Result<ValidatedTransfer, ShapeError> {
    if msg.instructions.is_empty() {
        return Err(ShapeError::NoInstructions);
    }

    let mut i = 0;
    while i < msg.instructions.len() && is_compute_budget(msg, i) {
        i += 1;
    }
    if i >= msg.instructions.len() {
        return Err(ShapeError::NoInstructions);
    }

    let ix = &msg.instructions[i];
    let program = msg.program_id(ix).ok_or(ShapeError::MissingAccounts)?;
    if program != &SYSTEM_PROGRAM {
        return Err(ShapeError::UnexpectedProgram);
    }
    if ix.accounts.len() < 2 {
        return Err(ShapeError::MissingAccounts);
    }
    if ix.data.len() < 12 {
        return Err(ShapeError::NotSystemTransfer);
    }
    let disc = u32::from_le_bytes(ix.data[0..4].try_into().unwrap());
    if disc != TRANSFER_DISCRIMINATOR {
        return Err(ShapeError::NotSystemTransfer);
    }
    let lamports = u64::from_le_bytes(ix.data[4..12].try_into().unwrap());
    if lamports > check.max_lamports {
        return Err(ShapeError::AmountAboveCap);
    }

    let from_idx = ix.accounts[0] as usize;
    let to_idx = ix.accounts[1] as usize;
    let from = *msg.account_keys.get(from_idx).ok_or(ShapeError::MissingAccounts)?;
    let to = *msg.account_keys.get(to_idx).ok_or(ShapeError::MissingAccounts)?;

    if from_idx != 0 {
        return Err(ShapeError::FeePayerNotSource);
    }
    if !msg.is_signer(from_idx) {
        return Err(ShapeError::SourceNotSigner);
    }
    if !msg.is_writable(to_idx) {
        return Err(ShapeError::DestinationNotWritable);
    }
    if to != check.expected_destination {
        return Err(ShapeError::WrongDestination);
    }

    for j in (i + 1)..msg.instructions.len() {
        if !is_compute_budget(msg, j) {
            return Err(ShapeError::ExtraInstruction);
        }
    }

    Ok(ValidatedTransfer {
        from,
        to,
        lamports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solana::message::{
        serialize_legacy_message, CompiledInstruction, LegacyMessage, MessageHeader,
    };
    use crate::solana::parse_legacy_message;
    use near_sdk::env;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn key(tag: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = tag;
        k[31] = tag;
        k
    }

    fn transfer_data(lamports: u64) -> Vec<u8> {
        let mut data = vec![2, 0, 0, 0];
        data.extend_from_slice(&lamports.to_le_bytes());
        data
    }

    fn transfer_message(from: [u8; 32], to: [u8; 32], lamports: u64) -> LegacyMessage {
        LegacyMessage {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed: 0,
                num_readonly_unsigned: 1,
            },
            account_keys: vec![from, to, SYSTEM_PROGRAM],
            recent_blockhash: key(0xff),
            instructions: vec![CompiledInstruction {
                program_id_index: 2,
                accounts: vec![0, 1],
                data: transfer_data(lamports),
            }],
        }
    }

    #[test]
    fn accepts_capped_transfer_to_fixed_destination() {
        let from = key(1);
        let to = key(2);
        let msg = transfer_message(from, to, 50_000);
        let got = validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: to,
                max_lamports: 100_000,
            },
        )
        .unwrap();
        assert_eq!(got.from, from);
        assert_eq!(got.to, to);
        assert_eq!(got.lamports, 50_000);
    }

    #[test]
    fn rejects_attacker_destination() {
        let msg = transfer_message(key(1), key(9), 1);
        let err = validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: key(2),
                max_lamports: 1_000,
            },
        )
        .unwrap_err();
        assert_eq!(err, ShapeError::WrongDestination);
    }

    #[test]
    fn rejects_amount_above_cap() {
        let to = key(2);
        let msg = transfer_message(key(1), to, 2_000);
        let err = validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: to,
                max_lamports: 1_000,
            },
        )
        .unwrap_err();
        assert_eq!(err, ShapeError::AmountAboveCap);
    }

    #[test]
    fn rejects_extra_instruction() {
        let to = key(2);
        let mut msg = transfer_message(key(1), to, 10);
        msg.instructions.push(CompiledInstruction {
            program_id_index: 2,
            accounts: vec![0, 1],
            data: transfer_data(1),
        });
        let err = validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: to,
                max_lamports: 100,
            },
        )
        .unwrap_err();
        assert_eq!(err, ShapeError::ExtraInstruction);
    }

    #[test]
    fn rejects_versioned_via_parser() {
        let mut bytes = serialize_legacy_message(&transfer_message(key(1), key(2), 1));
        bytes[0] |= 0x80;
        assert!(parse_legacy_message(&bytes).is_err());
    }

    #[test]
    fn mock_runtime_gas_for_parse_and_check() {
        let mut ctx = VMContextBuilder::new();
        testing_env!(ctx.prepaid_gas(near_sdk::Gas::from_tgas(30)).build());
        let to = key(2);
        let bytes = serialize_legacy_message(&transfer_message(key(1), to, 42));
        let before = env::used_gas();
        let msg = parse_legacy_message(&bytes).unwrap();
        let _ = validate_system_transfer(
            &msg,
            &TransferCheck {
                expected_destination: to,
                max_lamports: 100,
            },
        )
        .unwrap();
        let used = env::used_gas().saturating_sub(before);
        // Host-side mock runtime; recorded so spikes.md can quote a number.
        env::log_str(&format!(
            "parser_gas_estimate_tgas={}",
            used.as_tgas()
        ));
        env::log_str(&format!("parser_gas_estimate_gas={}", used.as_gas()));
        assert!(used.as_gas() > 0);
    }
}
