//! Cap, overflow, sweep and refund arithmetic. Pure functions so the state
//! machine can be tested without a VM.

use crate::TrustState;

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
        TrustState::Launched | TrustState::Distributed => 0,
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
/// Solana system-account rent-exempt minimum. A transfer that would leave
/// `0 < leftover < RENT_EXEMPT_MIN` cannot land; refund the full remaining
/// balance instead (close-out).
pub const RENT_EXEMPT_MIN: u64 = 890_880;

/// If an otherwise-valid refund would leave a live account below rent-exempt,
/// boost to the full remaining balance so the source can go to zero.
pub fn apply_rent_dust_closeout(eligible: u64, remaining_on_account: u64) -> u64 {
    if eligible == 0 {
        return 0;
    }
    let leftover = remaining_on_account.saturating_sub(eligible);
    if remaining_on_account >= RENT_EXEMPT_MIN && leftover > 0 && leftover < RENT_EXEMPT_MIN {
        remaining_on_account
    } else {
        eligible
    }
}

/// Raw base units of a 1_000_000_000 supply at 6 decimals (the pinned DBC
/// curve). The watcher-reported `tokens_bought` cannot exceed this.
pub const MAX_TOKENS_BOUGHT: u64 = 1_000_000_000 * 1_000_000;

/// `floor(tokens_bought * credited / credited_total)`.
/// Dust (`tokens_bought − Σ floors`) stays in the raise ATA. It is not a
/// share and is not claimable. Each floor's remainder is < 1 raw unit, so
/// dust is strictly less than the number of credited contributors.
pub fn pro_rata_share(tokens_bought: u64, credited: u64, credited_total: u64) -> u64 {
    if tokens_bought == 0 || credited == 0 || credited_total == 0 {
        return 0;
    }
    ((tokens_bought as u128) * (credited as u128) / (credited_total as u128)) as u64
}

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
        assert_eq!(refundable_lamports(&TrustState::Raising, 10, 40, 0, 0), 40);
        assert_eq!(refundable_lamports(&TrustState::Raising, 10, 40, 0, 15), 25);
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
        assert_eq!(
            refundable_lamports(&TrustState::Distributed, 10, 5, 0, 0),
            0
        );
    }

    #[test]
    fn pro_rata_floors_and_leaves_dust() {
        assert_eq!(pro_rata_share(0, 1, 1), 0);
        assert_eq!(pro_rata_share(100, 0, 100), 0);
        assert_eq!(pro_rata_share(1_000, 1, 2), 500);
        let a = pro_rata_share(100, 1, 3);
        let b = pro_rata_share(100, 2, 3);
        assert_eq!(a, 33);
        assert_eq!(b, 66);
        assert_eq!(100 - a - b, 1);
        assert!(pro_rata_share(MAX_TOKENS_BOUGHT, 1, 1) == MAX_TOKENS_BOUGHT);
    }

    #[test]
    fn raise_refund_only_in_refunding() {
        assert_eq!(raise_refundable_lamports(&TrustState::Executing, 10, 0), 0);
        assert_eq!(raise_refundable_lamports(&TrustState::Refunding, 10, 3), 7);
    }

    #[test]
    fn rent_dust_closeout_only_when_leftover_is_below_rent() {
        assert_eq!(apply_rent_dust_closeout(200_000, 1_000_000), 1_000_000);
        assert_eq!(apply_rent_dust_closeout(40, 50), 40);
        assert_eq!(apply_rent_dust_closeout(40, 2_000_000), 40);
        assert_eq!(apply_rent_dust_closeout(0, 1_000_000), 0);
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
