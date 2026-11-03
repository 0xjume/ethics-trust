//! Allowed Solana transaction shapes. Milestone 2: System transfer (A/B).

pub mod transfer;

pub use transfer::{validate_system_transfer, TransferCheck, ValidatedTransfer, SYSTEM_PROGRAM};
