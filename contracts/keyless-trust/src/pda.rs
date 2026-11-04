//! Solana program-derived addresses. Same algorithm as
//! `PublicKey.findProgramAddressSync`: sha256(seeds || bump || program ||
//! "ProgramDerivedAddress") and the first bump (255..0) that is off the
//! ed25519 curve.

use curve25519_dalek::edwards::CompressedEdwardsY;
use sha2::{Digest, Sha256};

const PDA_MARKER: &[u8] = b"ProgramDerivedAddress";

pub fn is_on_curve(bytes: &[u8; 32]) -> bool {
    CompressedEdwardsY(*bytes).decompress().is_some()
}

pub fn find_program_address(seeds: &[&[u8]], program_id: &[u8; 32]) -> [u8; 32] {
    for bump in (0u8..=255).rev() {
        let mut h = Sha256::new();
        for seed in seeds {
            h.update(seed);
        }
        h.update([bump]);
        h.update(program_id);
        h.update(PDA_MARKER);
        let digest: [u8; 32] = h.finalize().into();
        if !is_on_curve(&digest) {
            return digest;
        }
    }
    // Solana panics if no bump exists; treat as all-zero (tests cover real PDAs).
    [0u8; 32]
}

/// Larger mint first, then the other — `Buffer.compare === 1` in the SDK.
pub fn ordered_keys<'a>(a: &'a [u8; 32], b: &'a [u8; 32]) -> (&'a [u8; 32], &'a [u8; 32]) {
    if a.as_slice() > b.as_slice() {
        (a, b)
    } else {
        (b, a)
    }
}

#[cfg(test)]
mod tests {
    use crate::bs58;
    use crate::shapes::dbc::{
        derive_dbc_event_authority, derive_dbc_pool, derive_dbc_token_vault, derive_metadata,
        derive_token_ata, DBC_PROGRAM, WSOL_MINT,
    };

    fn key(tag: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = tag;
        k[31] = tag;
        k
    }

    #[test]
    fn pool_matches_sdk_derive_dbc_pool_address() {
        let pool = derive_dbc_pool(&key(4), &key(2));
        assert_eq!(
            bs58::encode(&pool),
            "Fu5mNnQDCYGXAZAQiV6rj5ad1GUiAcNWkU8qHJkM1FmS"
        );
    }

    #[test]
    fn event_authority_matches_sdk() {
        assert_eq!(
            bs58::encode(&derive_dbc_event_authority()),
            "8Ks12pbrD6PXxfty1hVQiE9sc289zgU1zHkvXhrSdriF"
        );
    }

    #[test]
    fn token_vaults_and_atas_match_sdk() {
        let pool = derive_dbc_pool(&key(4), &key(2));
        let base = derive_dbc_token_vault(&pool, &key(2));
        let quote = derive_dbc_token_vault(&pool, &WSOL_MINT);
        let wsol_ata = derive_token_ata(&key(1), &WSOL_MINT);
        let token_ata = derive_token_ata(&key(1), &key(2));
        let meta = derive_metadata(&key(2));
        assert_eq!(
            bs58::encode(&base),
            "B7VJTGtefiaFExkB3EBqvV9KDsza8AawXAatMKUGWKkL"
        );
        assert_eq!(
            bs58::encode(&quote),
            "FTZHchvts5siCBq83F1u2QubWxNJhtjMum3JginJCom5"
        );
        assert_eq!(
            bs58::encode(&wsol_ata),
            "DBLdBAHF35ZRH3LksLMYJLNy4Uf2ozCxjsUab5o5KFX9"
        );
        assert_eq!(
            bs58::encode(&token_ata),
            "9jwmjGE7AgAD5a55Y44gVAEWEBGRHP9qGY5zgPnPebuf"
        );
        assert_eq!(
            bs58::encode(&meta),
            "FESj72f38fE8EkrwErpM64Eo4GGW7adMuwNehtAh4QU2"
        );
        let _ = DBC_PROGRAM;
    }
}
