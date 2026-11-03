//! Solana compact-u16 / shortvec encoding (legacy + v0 messages).

use super::error::ParseError;

/// Read a compact-u16 and advance `offset`.
pub fn read_compact_u16(data: &[u8], offset: &mut usize) -> Result<u16, ParseError> {
    let mut value: u32 = 0;
    let mut shift = 0;
    for _ in 0..3 {
        if *offset >= data.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let byte = data[*offset];
        *offset += 1;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            if value > u32::from(u16::MAX) {
                return Err(ParseError::CompactU16Overflow);
            }
            return Ok(value as u16);
        }
        shift += 7;
        if shift >= 21 {
            return Err(ParseError::CompactU16Overflow);
        }
    }
    Err(ParseError::CompactU16Overflow)
}

/// Encode `n` as compact-u16.
pub fn write_compact_u16(n: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    let mut rem = u32::from(n);
    loop {
        let mut elem = (rem & 0x7f) as u8;
        rem >>= 7;
        if rem != 0 {
            elem |= 0x80;
        }
        out.push(elem);
        if rem == 0 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_u16_roundtrip_small() {
        for n in [0u16, 1, 127, 128, 255, 16383, 16384, u16::MAX] {
            let encoded = write_compact_u16(n);
            let mut offset = 0;
            assert_eq!(read_compact_u16(&encoded, &mut offset).unwrap(), n);
            assert_eq!(offset, encoded.len());
        }
    }

    #[test]
    fn compact_u16_rejects_truncated() {
        let mut offset = 0;
        assert_eq!(
            read_compact_u16(&[0x80], &mut offset),
            Err(ParseError::UnexpectedEof)
        );
    }
}
