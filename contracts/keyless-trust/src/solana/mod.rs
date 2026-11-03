//! Solana message parsing used by the Trust contract shape checkers.

pub mod compact;
pub mod error;
pub mod message;

pub use error::{ParseError, ShapeError};
pub use message::{
    parse_legacy_message, serialize_legacy_message, CompiledInstruction, LegacyMessage,
    MessageHeader,
};
