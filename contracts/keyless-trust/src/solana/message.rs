//! Legacy Solana message parser. Versioned messages and ALTs are rejected.

use super::compact::{read_compact_u16, write_compact_u16};
use super::error::ParseError;

pub const PUBKEY_LEN: usize = 32;
pub const MAX_ACCOUNT_KEYS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHeader {
    pub num_required_signatures: u8,
    pub num_readonly_signed: u8,
    pub num_readonly_unsigned: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInstruction {
    pub program_id_index: u8,
    pub accounts: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyMessage {
    pub header: MessageHeader,
    pub account_keys: Vec<[u8; PUBKEY_LEN]>,
    pub recent_blockhash: [u8; PUBKEY_LEN],
    pub instructions: Vec<CompiledInstruction>,
}

impl LegacyMessage {
    pub fn program_id(&self, ix: &CompiledInstruction) -> Option<&[u8; PUBKEY_LEN]> {
        self.account_keys.get(ix.program_id_index as usize)
    }

    pub fn is_writable(&self, index: usize) -> bool {
        let signed = self.header.num_required_signatures as usize;
        let ro_signed = self.header.num_readonly_signed as usize;
        let ro_unsigned = self.header.num_readonly_unsigned as usize;
        let n = self.account_keys.len();
        if index < signed {
            index < signed.saturating_sub(ro_signed)
        } else if index < n {
            index < n.saturating_sub(ro_unsigned)
        } else {
            false
        }
    }

    pub fn is_signer(&self, index: usize) -> bool {
        index < self.header.num_required_signatures as usize
    }
}

/// Parse a serialized legacy Solana message. Rejects v0 (`0x80`) and v1.
pub fn parse_legacy_message(bytes: &[u8]) -> Result<LegacyMessage, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::EmptyMessage);
    }
    if bytes[0] & 0x80 != 0 {
        return Err(ParseError::VersionedMessage);
    }
    if bytes.len() < 3 {
        return Err(ParseError::UnexpectedEof);
    }

    let header = MessageHeader {
        num_required_signatures: bytes[0],
        num_readonly_signed: bytes[1],
        num_readonly_unsigned: bytes[2],
    };
    let mut offset = 3;

    let num_keys = read_compact_u16(bytes, &mut offset)? as usize;
    if num_keys > MAX_ACCOUNT_KEYS {
        return Err(ParseError::TooManyAccounts);
    }
    let mut account_keys = Vec::with_capacity(num_keys);
    for _ in 0..num_keys {
        if offset + PUBKEY_LEN > bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let mut key = [0u8; PUBKEY_LEN];
        key.copy_from_slice(&bytes[offset..offset + PUBKEY_LEN]);
        account_keys.push(key);
        offset += PUBKEY_LEN;
    }

    if offset + PUBKEY_LEN > bytes.len() {
        return Err(ParseError::UnexpectedEof);
    }
    let mut recent_blockhash = [0u8; PUBKEY_LEN];
    recent_blockhash.copy_from_slice(&bytes[offset..offset + PUBKEY_LEN]);
    offset += PUBKEY_LEN;

    let num_ixs = read_compact_u16(bytes, &mut offset)? as usize;
    let mut instructions = Vec::with_capacity(num_ixs);
    for _ in 0..num_ixs {
        if offset >= bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let program_id_index = bytes[offset];
        offset += 1;
        let n_acc = read_compact_u16(bytes, &mut offset)? as usize;
        if offset + n_acc > bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let accounts = bytes[offset..offset + n_acc].to_vec();
        offset += n_acc;
        let data_len = read_compact_u16(bytes, &mut offset)? as usize;
        if offset + data_len > bytes.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let data = bytes[offset..offset + data_len].to_vec();
        offset += data_len;
        instructions.push(CompiledInstruction {
            program_id_index,
            accounts,
            data,
        });
    }

    if offset != bytes.len() {
        return Err(ParseError::TrailingBytes);
    }

    Ok(LegacyMessage {
        header,
        account_keys,
        recent_blockhash,
        instructions,
    })
}

/// Build a legacy message (test / fixture helper).
pub fn serialize_legacy_message(message: &LegacyMessage) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(message.header.num_required_signatures);
    out.push(message.header.num_readonly_signed);
    out.push(message.header.num_readonly_unsigned);
    out.extend_from_slice(&write_compact_u16(message.account_keys.len() as u16));
    for key in &message.account_keys {
        out.extend_from_slice(key);
    }
    out.extend_from_slice(&message.recent_blockhash);
    out.extend_from_slice(&write_compact_u16(message.instructions.len() as u16));
    for ix in &message.instructions {
        out.push(ix.program_id_index);
        out.extend_from_slice(&write_compact_u16(ix.accounts.len() as u16));
        out.extend_from_slice(&ix.accounts);
        out.extend_from_slice(&write_compact_u16(ix.data.len() as u16));
        out.extend_from_slice(&ix.data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[31] = n;
        k
    }

    fn sample() -> LegacyMessage {
        LegacyMessage {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed: 0,
                num_readonly_unsigned: 1,
            },
            account_keys: vec![key(1), key(2), key(3)],
            recent_blockhash: key(9),
            instructions: vec![CompiledInstruction {
                program_id_index: 2,
                accounts: vec![0, 1],
                data: vec![2, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0, 0],
            }],
        }
    }

    #[test]
    fn roundtrip_legacy_message() {
        let msg = sample();
        let bytes = serialize_legacy_message(&msg);
        assert_eq!(parse_legacy_message(&bytes).unwrap(), msg);
    }

    #[test]
    fn rejects_versioned_prefix() {
        let mut bytes = serialize_legacy_message(&sample());
        bytes[0] |= 0x80;
        assert_eq!(
            parse_legacy_message(&bytes),
            Err(ParseError::VersionedMessage)
        );
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(parse_legacy_message(&[]), Err(ParseError::EmptyMessage));
    }

    #[test]
    fn writable_and_signer_layout() {
        let msg = sample();
        assert!(msg.is_signer(0));
        assert!(msg.is_writable(0));
        assert!(msg.is_writable(1));
        assert!(!msg.is_writable(2));
    }
}
