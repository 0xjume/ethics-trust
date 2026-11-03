//! Allowed Solana transaction shapes. Day-1 spike: System transfer only.

pub mod transfer;

pub use transfer::{validate_system_transfer, TransferCheck, ValidatedTransfer, SYSTEM_PROGRAM};
