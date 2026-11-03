//! Cap, overflow, sweep and refund arithmetic. Pure functions so the state
//! machine can be tested without a VM.

use crate::TrustState;

pub const DEFAULT_CAP_BPS: u16 = 100;
pub const DEFAULT_FILL_FEE_BPS: u16 = 150;
pub const BPS_DENOM: u64 = 10_000;
pub const EXECUTION_TIMEOUT_NS: u64 = 24 * 60 * 60 * 1_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepositSplit {
    pub credited: u64,
    pub overflow: u64,
}

/// `target * cap_bps / 10_000`. 100 bps = 1% of target per wallet.
pub fn per_wallet_cap(target_lamports: u64, cap_bps: u16) -> u64 {
    ((target_lamports as u128) * (cap_bps as u128) / (BPS_DENOM as u128)) as u64
}

/// Fill fee taken from the raise at DBC launch (next milestone). Stored now.
pub fn fill_fee_lamports(credited_total: u64, fill_fee_bps: u16) -> u64 {
    ((credited_total as u128) * (fill_fee_bps as u128) / (BPS_DENOM as u128)) as u64
}

/// Credited is min(amount, remaining per-wallet cap, remaining target).
/// Anything else is refundable overflow. The watcher cannot choose the split.
pub fn split_deposit(
    amount: u64,
    already_credited: u64,
    credited_total: u64,
    target_lamports: u64,
    cap_bps: u16,
) -> DepositSplit {
    if amount == 0 {
        return DepositSplit {
            credited: 0,
            overflow: 0,
        };
    }
    let cap = per_wallet_cap(target_lamports, cap_bps);
    let remaining_cap = cap.saturating_sub(already_credited);
    let remaining_target = target_lamports.saturating_sub(credited_total);
    let credited = amount.min(remaining_cap).min(remaining_target);
    DepositSplit {
        credited,
        overflow: amount - credited,
    }
}

/// Overflow is never part of the raise, so it is refundable in Raising,
/// Filled and Executing. Refunding: credited + overflow − swept.
/// Launched: nothing. Already-refunded lamports are subtracted.
pub fn refundable_lamports(
    state: &TrustState,
    credited: u64,
    overflow: u64,
    swept: u64,
    refunded: u64,
) -> u64 {
    let eligible = match state {
        TrustState::Raising | TrustState::Filled | TrustState::Executing => overflow,
        TrustState::Refunding => credited.saturating_add(overflow).saturating_sub(swept),
        TrustState::Launched => 0,
    };
    eligible.saturating_sub(refunded)
}

/// Swept SOL sitting in the raise vault, refundable only in Refunding.
pub fn raise_refundable_lamports(state: &TrustState, swept: u64, raise_refunded: u64) -> u64 {
    match state {
        TrustState::Refunding => swept.saturating_sub(raise_refunded),
        _ => 0,
    }
}

/// Solana `recent_blockhash` lives ~150 slots (≈60–90 s). 150 s is past
/// that window, so a pending that can be `release_pending`'d can no
/// longer land on Solana. A late honest confirm (no re-sign) still books.
pub const PENDING_EXPIRY_NS: u64 = 150 * 1_000_000_000;

/// Sweep only after the raise is filled. Never more than credited − swept,
/// so a wallet cannot push more than 1% of target into the raise vault.
pub fn sweepable_lamports(state: &TrustState, credited: u64, swept: u64) -> u64 {
    match state {
        TrustState::Filled | TrustState::Executing => credited.saturating_sub(swept),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_percent_of_two_sol() {
        assert_eq!(per_wallet_cap(2_000_000_000, 100), 20_000_000);
    }

    #[test]
    fn fill_fee_is_one_and_a_half_percent() {
        assert_eq!(fill_fee_lamports(2_000_000_000, 150), 30_000_000);
    }

    #[test]
    fn overflow_when_over_cap() {
        let split = split_deposit(50, 0, 0, 1_000, 100);
        assert_eq!(
            split,
            DepositSplit {
                credited: 10,
                overflow: 40
            }
        );
    }

    #[test]
    fn overflow_when_target_is_full() {
        let split = split_deposit(50, 0, 1_000, 1_000, 100);
        assert_eq!(
            split,
            DepositSplit {
                credited: 0,
                overflow: 50
            }
        );
    }

    #[test]
    fn remaining_cap_is_respected_across_reports() {
        let first = split_deposit(6, 0, 0, 1_000, 100);
        assert_eq!(first.credited, 6);
        let second = split_deposit(6, first.credited, first.credited, 1_000, 100);
        assert_eq!(
            second,
            DepositSplit {
                credited: 4,
                overflow: 2
            }
        );
    }

    #[test]
    fn raising_refunds_overflow_only() {
        assert_eq!(
            refundable_lamports(&TrustState::Raising, 10, 40, 0, 0),
            40
        );
        assert_eq!(
            refundable_lamports(&TrustState::Raising, 10, 40, 0, 15),
            25
        );
    }

    #[test]
    fn refunding_returns_everything_not_swept() {
        assert_eq!(
            refundable_lamports(&TrustState::Refunding, 10, 40, 3, 0),
            47
        );
        assert_eq!(
            refundable_lamports(&TrustState::Refunding, 10, 40, 3, 47),
            0
        );
    }

    #[test]
    fn filled_and_executing_refund_overflow_only() {
        assert_eq!(refundable_lamports(&TrustState::Filled, 10, 5, 0, 0), 5);
        assert_eq!(refundable_lamports(&TrustState::Executing, 10, 5, 0, 2), 3);
        assert_eq!(refundable_lamports(&TrustState::Launched, 10, 5, 0, 0), 0);
    }

    #[test]
    fn raise_refund_only_in_refunding() {
        assert_eq!(raise_refundable_lamports(&TrustState::Executing, 10, 0), 0);
        assert_eq!(raise_refundable_lamports(&TrustState::Refunding, 10, 3), 7);
    }

    #[test]
    fn pending_expiry_outlives_solana_blockhash() {
        assert_eq!(PENDING_EXPIRY_NS, 150 * 1_000_000_000);
        assert!(PENDING_EXPIRY_NS > 90 * 1_000_000_000);
    }

    #[test]
    fn sweep_only_in_filled_or_executing() {
        assert_eq!(sweepable_lamports(&TrustState::Raising, 10, 0), 0);
        assert_eq!(sweepable_lamports(&TrustState::Filled, 10, 3), 7);
        assert_eq!(sweepable_lamports(&TrustState::Executing, 10, 10), 0);
        assert_eq!(sweepable_lamports(&TrustState::Refunding, 10, 0), 0);
    }
}
