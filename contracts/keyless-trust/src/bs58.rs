//! Minimal Bitcoin/Solana Base58 (no checksum). Used to turn contributor
//! pubkeys and derived vault addresses into 32-byte keys without a crate.

const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

pub fn decode(input: &str) -> Result<Vec<u8>, &'static str> {
    if input.is_empty() {
        return Err("empty base58");
    }
    let mut bytes = vec![0u8; input.len()];
    let mut length = 0usize;
    for ch in input.bytes() {
        let mut carry = ALPHABET
            .iter()
            .position(|&c| c == ch)
            .ok_or("invalid base58 character")? as u32;
        for b in bytes.iter_mut().take(length) {
            carry += 58 * (*b as u32);
            *b = (carry % 256) as u8;
            carry /= 256;
        }
        while carry > 0 {
            if length >= bytes.len() {
                return Err("base58 overflow");
            }
            bytes[length] = (carry % 256) as u8;
            length += 1;
            carry /= 256;
        }
    }
    let leading_zeros = input.bytes().take_while(|&c| c == b'1').count();
    let mut out = vec![0u8; leading_zeros];
    out.extend(bytes[..length].iter().rev());
    Ok(out)
}

pub fn decode_pubkey(input: &str) -> Result<[u8; 32], &'static str> {
    let bytes = decode(input)?;
    bytes
        .try_into()
        .map_err(|_| "solana pubkey must decode to 32 bytes")
}

pub fn encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut digits = vec![0u8; data.len() * 2];
    let mut length = 0usize;
    for &byte in data {
        let mut carry = byte as u32;
        for d in digits.iter_mut().take(length) {
            carry += (*d as u32) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits[length] = (carry % 58) as u8;
            length += 1;
            carry /= 58;
        }
    }
    let leading_zeros = data.iter().take_while(|&&b| b == 0).count();
    let mut out = String::from_utf8(vec![b'1'; leading_zeros]).unwrap();
    for d in digits[..length].iter().rev() {
        out.push(ALPHABET[*d as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_program_is_ones() {
        assert_eq!(encode(&[0u8; 32]), "11111111111111111111111111111111");
        assert_eq!(decode_pubkey("11111111111111111111111111111111").unwrap(), [0u8; 32]);
    }

    #[test]
    fn roundtrip_nonzero() {
        let mut key = [0u8; 32];
        key[0] = 7;
        key[31] = 9;
        let encoded = encode(&key);
        assert_eq!(decode_pubkey(&encoded).unwrap(), key);
    }

    #[test]
    fn known_raise_example() {
        let pk = decode_pubkey("B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj").unwrap();
        assert_eq!(encode(&pk), "B4paagQ87U6kmpn3AoXYaZMQVsFCJbZtjh8tBoKDDXwj");
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(decode_pubkey("1").is_err());
    }
}
