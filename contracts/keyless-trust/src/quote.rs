//! First-buy quote for a brand-new DBC pool.
//!
//! Port of `@meteora-ag/dynamic-bonding-curve-sdk@2.0.0`
//! `getQuoteFromInputAmount` → `quoteAtInitialization` → `swapQuoteExactIn`
//! → `getSwapResultFromExactInput` → `calculateQuoteToBaseFromAmountIn`,
//! quote → base, exact-in, no referral, no Token-2022 transfer fee.
//!
//! A fresh pool has `sqrtPrice = sqrtStartPrice`, quote reserve 0, `hasSwap = 0`,
//! and volatility 0, so the dynamic fee is 0. `enableFirstSwapWithMinFee`
//! uses the linear scheduler's minimum numerator
//! (`cliff − numberOfPeriod × reductionFactor`). With the flag off the first
//! buy is still period 0, so the numerator is the cliff. `periodFrequency`
//! does not change that quote and is not stored.
//!
//! Rounding matches the SDK: `mulDiv` ceil/floor on full 256-bit products,
//! quote delta divides by 2^128, base delta divides by `lower × upper`.
//! Exponential schedulers and the rate limiter are rejected.

use near_sdk::json_types::U128;
use near_sdk::near;

use crate::math::MAX_TOKENS_BOUGHT;

const FEE_DENOMINATOR: u64 = 1_000_000_000;
const MAX_FEE_NUMERATOR: u64 = 990_000_000;
const MAX_CURVE_POINTS: usize = 16;

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurvePoint {
    pub sqrt_price: U128,
    pub liquidity: U128,
}

/// Curve the contract quotes. Committed into `params_hash`.
/// `collect_fee_mode`: 0 = quote token (fee on input for quote→base),
/// 1 = output token (fee on base). `base_fee_mode` 0 = linear only.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbcCurve {
    pub sqrt_start_price: U128,
    pub migration_quote_threshold: u64,
    pub collect_fee_mode: u8,
    pub enable_first_swap_with_min_fee: bool,
    pub base_fee_mode: u8,
    pub cliff_fee_numerator: u64,
    pub number_of_period: u16,
    pub reduction_factor: u64,
    pub points: Vec<CurvePoint>,
}

#[derive(Clone, Copy)]
enum Round {
    Down,
    Up,
}

/// Little-endian 64-bit limbs. Value = Σ limb[i] × 2^(64i).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct U256([u64; 4]);

impl U256 {
    const fn zero() -> Self {
        Self([0; 4])
    }

    fn from_u64(v: u64) -> Self {
        Self([v, 0, 0, 0])
    }

    fn from_u128(v: u128) -> Self {
        Self([v as u64, (v >> 64) as u64, 0, 0])
    }

    fn is_zero(self) -> bool {
        self.0.iter().all(|l| *l == 0)
    }

    fn low128_is_zero(self) -> bool {
        self.0[0] == 0 && self.0[1] == 0
    }

    fn bit(self, i: u32) -> bool {
        ((self.0[(i / 64) as usize] >> (i % 64)) & 1) == 1
    }

    fn set_bit(&mut self, i: u32) {
        self.0[(i / 64) as usize] |= 1u64 << (i % 64);
    }

    fn cmp(self, other: Self) -> core::cmp::Ordering {
        self.0.iter().rev().cmp(other.0.iter().rev())
    }

    fn shr128(self) -> Self {
        Self([self.0[2], self.0[3], 0, 0])
    }

    /// `amount << 128`. `amount` fits in one limb, so the shift is exact.
    fn shl128_u64(amount: u64) -> Self {
        Self([0, 0, amount, 0])
    }

    fn add_one(self) -> Option<Self> {
        let mut out = self.0;
        let mut carry = 1u64;
        for limb in &mut out {
            let (v, c) = limb.overflowing_add(carry);
            *limb = v;
            if !c {
                return Some(Self(out));
            }
            carry = 1;
        }
        None
    }

    fn to_u64(self) -> Option<u64> {
        if self.0[1] | self.0[2] | self.0[3] == 0 {
            Some(self.0[0])
        } else {
            None
        }
    }

    fn to_u128(self) -> Option<u128> {
        if self.0[2] | self.0[3] == 0 {
            Some((self.0[0] as u128) | ((self.0[1] as u128) << 64))
        } else {
            None
        }
    }

    fn mul_u128(a: u128, b: u128) -> Self {
        let a0 = a as u64 as u128;
        let a1 = a >> 64;
        let b0 = b as u64 as u128;
        let b1 = b >> 64;
        let p00 = a0 * b0;
        let p01 = a0 * b1;
        let p10 = a1 * b0;
        let p11 = a1 * b1;
        let mid = (p00 >> 64) + (p01 & 0xffff_ffff_ffff_ffff) + (p10 & 0xffff_ffff_ffff_ffff);
        let lo = (p00 & 0xffff_ffff_ffff_ffff) | ((mid & 0xffff_ffff_ffff_ffff) << 64);
        let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
        Self([lo as u64, (lo >> 64) as u64, hi as u64, (hi >> 64) as u64])
    }

    fn div_mod(self, denom: Self) -> Result<(Self, Self), &'static str> {
        if denom.is_zero() {
            return Err("division by zero");
        }
        let mut quot = Self::zero();
        let mut rem = [0u64; 5];
        for i in (0..256).rev() {
            shl1_wide(&mut rem);
            if self.bit(i) {
                rem[0] |= 1;
            }
            if ge_wide(&rem, denom) {
                sub_wide(&mut rem, denom);
                quot.set_bit(i);
            }
        }
        Ok((quot, Self([rem[0], rem[1], rem[2], rem[3]])))
    }

    fn div_floor(self, denom: Self) -> Result<Self, &'static str> {
        Ok(self.div_mod(denom)?.0)
    }

    fn div_ceil(self, denom: Self) -> Result<Self, &'static str> {
        let (quot, rem) = self.div_mod(denom)?;
        if rem.is_zero() {
            Ok(quot)
        } else {
            quot.add_one().ok_or("ceil division overflow")
        }
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(*other))
    }
}

fn shl1_wide(r: &mut [u64; 5]) {
    let mut carry = 0u64;
    for limb in r.iter_mut() {
        let next = *limb >> 63;
        *limb = (*limb << 1) | carry;
        carry = next;
    }
}

fn ge_wide(r: &[u64; 5], d: U256) -> bool {
    if r[4] > 0 {
        return true;
    }
    U256([r[0], r[1], r[2], r[3]]) >= d
}

fn sub_wide(r: &mut [u64; 5], d: U256) {
    let mut borrow = 0u64;
    for i in 0..4 {
        let (a, b1) = r[i].overflowing_sub(d.0[i]);
        let (a, b2) = a.overflowing_sub(borrow);
        r[i] = a;
        borrow = u64::from(b1 || b2);
    }
    r[4] = r[4].wrapping_sub(borrow);
}

fn excluded_fee(amount: u64, numerator: u64) -> Result<u64, &'static str> {
    let prod = (amount as u128)
        .checked_mul(numerator as u128)
        .ok_or("fee product overflow")?;
    let den = FEE_DENOMINATOR as u128;
    let fee = prod.div_ceil(den);
    if fee > amount as u128 {
        return Err("fee exceeds amount");
    }
    Ok((amount as u128 - fee) as u64)
}

fn first_buy_fee_numerator(curve: &DbcCurve) -> Result<u64, &'static str> {
    if curve.base_fee_mode != 0 {
        return Err("only a linear fee scheduler is quoted");
    }
    if curve.cliff_fee_numerator > MAX_FEE_NUMERATOR {
        return Err("fee numerator above max");
    }
    let fee = if curve.enable_first_swap_with_min_fee {
        let cut = (curve.number_of_period as u64)
            .checked_mul(curve.reduction_factor)
            .ok_or("fee reduction overflow")?;
        curve
            .cliff_fee_numerator
            .checked_sub(cut)
            .ok_or("fee reduction exceeds cliff")?
    } else {
        curve.cliff_fee_numerator
    };
    if fee > MAX_FEE_NUMERATOR {
        return Err("fee numerator above max");
    }
    Ok(fee)
}

fn delta_quote(
    lower: u128,
    upper: u128,
    liquidity: u128,
    round: Round,
) -> Result<U256, &'static str> {
    let delta = upper.checked_sub(lower).ok_or("sqrt price inverted")?;
    let prod = U256::mul_u128(liquidity, delta);
    match round {
        Round::Down => Ok(prod.shr128()),
        Round::Up => {
            if prod.low128_is_zero() {
                Ok(prod.shr128())
            } else {
                prod.shr128().add_one().ok_or("quote delta overflow")
            }
        }
    }
}

fn delta_base_u64(
    lower: u128,
    upper: u128,
    liquidity: u128,
    round: Round,
) -> Result<u64, &'static str> {
    let numerator = upper.checked_sub(lower).ok_or("sqrt price inverted")?;
    let denom = U256::mul_u128(lower, upper);
    let prod = U256::mul_u128(liquidity, numerator);
    let quot = match round {
        Round::Down => prod.div_floor(denom)?,
        Round::Up => prod.div_ceil(denom)?,
    };
    quot.to_u64().ok_or("base delta does not fit u64")
}

fn next_sqrt_quote_in(
    sqrt_price: u128,
    liquidity: u128,
    amount: u64,
) -> Result<u128, &'static str> {
    if sqrt_price == 0 || liquidity == 0 {
        return Err("sqrt price and liquidity must be > 0");
    }
    let quot = U256::shl128_u64(amount).div_floor(U256::from_u128(liquidity))?;
    let quot = quot.to_u128().ok_or("next sqrt overflow")?;
    sqrt_price.checked_add(quot).ok_or("next sqrt overflow")
}

fn migration_sqrt(curve: &DbcCurve) -> Result<u128, &'static str> {
    if curve.points.is_empty() {
        return Err("curve is empty");
    }
    let mut amount_left = curve.migration_quote_threshold;
    let mut sqrt_price = curve.sqrt_start_price.0;
    if sqrt_price == 0 {
        return Err("sqrt start price must be > 0");
    }
    for point in &curve.points {
        let p_sqrt = point.sqrt_price.0;
        let liquidity = point.liquidity.0;
        if p_sqrt == 0 || liquidity == 0 {
            break;
        }
        let max_amount = delta_quote(sqrt_price, p_sqrt, liquidity, Round::Up)?;
        if max_amount > U256::from_u64(amount_left) {
            return next_sqrt_quote_in(sqrt_price, liquidity, amount_left);
        }
        let max_u64 = max_amount
            .to_u64()
            .ok_or("migration segment does not fit u64")?;
        amount_left = amount_left
            .checked_sub(max_u64)
            .ok_or("migration amount underflow")?;
        sqrt_price = p_sqrt;
    }
    if amount_left != 0 {
        return Err("curve cannot hold the migration threshold");
    }
    Ok(sqrt_price)
}

fn quote_to_base(
    curve: &DbcCurve,
    mut sqrt_price: u128,
    mut amount_left: u64,
    stop_sqrt: u128,
) -> Result<(u64, u128, u64), &'static str> {
    let mut total = 0u64;
    for point in &curve.points {
        let p_sqrt = point.sqrt_price.0;
        let liquidity = point.liquidity.0;
        if p_sqrt == 0 || liquidity == 0 {
            break;
        }
        let reference = p_sqrt.min(stop_sqrt);
        if reference <= sqrt_price {
            continue;
        }
        let max_in = delta_quote(sqrt_price, reference, liquidity, Round::Up)?;
        if U256::from_u64(amount_left) < max_in {
            let next = next_sqrt_quote_in(sqrt_price, liquidity, amount_left)?;
            let base = delta_base_u64(sqrt_price, next, liquidity, Round::Down)?;
            total = total.checked_add(base).ok_or("output overflow")?;
            sqrt_price = next;
            amount_left = 0;
            break;
        }
        let base = delta_base_u64(sqrt_price, reference, liquidity, Round::Down)?;
        total = total.checked_add(base).ok_or("output overflow")?;
        sqrt_price = reference;
        let max_u64 = max_in.to_u64().ok_or("segment quote does not fit u64")?;
        amount_left = amount_left.checked_sub(max_u64).ok_or("amount underflow")?;
        if sqrt_price == stop_sqrt {
            break;
        }
    }
    Ok((total, sqrt_price, amount_left))
}

pub fn validate_curve(curve: &DbcCurve) -> Result<(), &'static str> {
    if curve.points.is_empty() || curve.points.len() > MAX_CURVE_POINTS {
        return Err("curve must have 1 to 16 points");
    }
    if curve.sqrt_start_price.0 == 0 {
        return Err("sqrt start price must be > 0");
    }
    if curve.migration_quote_threshold == 0 {
        return Err("migration threshold must be > 0");
    }
    if curve.collect_fee_mode > 1 {
        return Err("collect fee mode must be quote or output");
    }
    for point in &curve.points {
        if point.sqrt_price.0 == 0 || point.liquidity.0 == 0 {
            return Err("curve point sqrt price and liquidity must be > 0");
        }
    }
    let _ = first_buy_fee_numerator(curve)?;
    let _ = migration_sqrt(curve)?;
    Ok(())
}

/// Exact-in quote→base output for `amount_in` lamports on a brand-new pool.
/// Output is raw base units (6 decimals on the pinned curve).
pub fn quote_first_buy(curve: &DbcCurve, amount_in: u64) -> Result<u64, &'static str> {
    if amount_in == 0 {
        return Err("quote amount is zero");
    }
    validate_curve(curve)?;
    let fee_num = first_buy_fee_numerator(curve)?;
    let stop = migration_sqrt(curve)?;
    let swap_in = if curve.collect_fee_mode == 0 {
        let after = excluded_fee(amount_in, fee_num)?;
        if after == 0 {
            return Err("quote input is zero after fee");
        }
        after
    } else {
        amount_in
    };
    let (output, _, left) = quote_to_base(curve, curve.sqrt_start_price.0, swap_in, stop)?;
    if left != 0 {
        return Err("insufficient curve liquidity");
    }
    let out = if curve.collect_fee_mode == 1 {
        let after = excluded_fee(output, fee_num)?;
        if after == 0 {
            return Err("quote output is zero after fee");
        }
        after
    } else {
        output
    };
    if out == 0 || out > MAX_TOKENS_BOUGHT {
        return Err("quoted output out of range");
    }
    Ok(out)
}

/// Borsh layout of `DbcCurve`, appended to `params_hash`.
pub fn append_curve_bytes(buf: &mut Vec<u8>, curve: &DbcCurve) {
    buf.extend_from_slice(&curve.sqrt_start_price.0.to_le_bytes());
    buf.extend_from_slice(&curve.migration_quote_threshold.to_le_bytes());
    buf.push(curve.collect_fee_mode);
    buf.push(u8::from(curve.enable_first_swap_with_min_fee));
    buf.push(curve.base_fee_mode);
    buf.extend_from_slice(&curve.cliff_fee_numerator.to_le_bytes());
    buf.extend_from_slice(&curve.number_of_period.to_le_bytes());
    buf.extend_from_slice(&curve.reduction_factor.to_le_bytes());
    let n = curve.points.len() as u32;
    buf.extend_from_slice(&n.to_le_bytes());
    for point in &curve.points {
        buf.extend_from_slice(&point.sqrt_price.0.to_le_bytes());
        buf.extend_from_slice(&point.liquidity.0.to_le_bytes());
    }
}

#[cfg(test)]
pub(crate) fn demo_curve() -> DbcCurve {
    curve_from_fixture("demo")
}

#[cfg(test)]
fn curve_from_fixture(name: &str) -> DbcCurve {
    let raw: near_sdk::serde_json::Value =
        near_sdk::serde_json::from_str(include_str!("../tests/fixtures/dbc_quotes.json")).unwrap();
    let curve = raw["curves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    curve_from_value(curve)
}

#[cfg(test)]
fn u128_field(v: &near_sdk::serde_json::Value, key: &str) -> u128 {
    v[key].as_str().unwrap().parse().unwrap()
}

#[cfg(test)]
fn curve_from_value(v: &near_sdk::serde_json::Value) -> DbcCurve {
    let points = v["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| CurvePoint {
            sqrt_price: U128(u128_field(p, "sqrtPrice")),
            liquidity: U128(u128_field(p, "liquidity")),
        })
        .collect();
    DbcCurve {
        sqrt_start_price: U128(u128_field(v, "sqrtStartPrice")),
        migration_quote_threshold: v["migrationQuoteThreshold"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        collect_fee_mode: v["collectFeeMode"].as_u64().unwrap() as u8,
        enable_first_swap_with_min_fee: v["enableFirstSwapWithMinFee"].as_bool().unwrap(),
        base_fee_mode: v["baseFeeMode"].as_u64().unwrap() as u8,
        cliff_fee_numerator: v["cliffFeeNumerator"].as_str().unwrap().parse().unwrap(),
        number_of_period: v["numberOfPeriod"].as_u64().unwrap() as u16,
        reduction_factor: v["reductionFactor"].as_str().unwrap().parse().unwrap(),
        points,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u256_mul_and_shift_match_schoolbook() {
        let prod = U256::mul_u128(3, 5);
        assert_eq!(prod.to_u64(), Some(15));
        let shifted = U256::shl128_u64(3);
        let half = shifted.div_floor(U256::from_u64(2)).unwrap();
        // (3 << 128) / 2 = 3 << 127, which is 2^127 + 2^128.
        assert_eq!(half.0, [0, 1 << 63, 1, 0]);
        let ceil = U256::from_u64(5).div_ceil(U256::from_u64(2)).unwrap();
        assert_eq!(ceil.to_u64(), Some(3));
    }

    #[test]
    fn golden_quotes_match_sdk_2_0_0() {
        let raw: near_sdk::serde_json::Value =
            near_sdk::serde_json::from_str(include_str!("../tests/fixtures/dbc_quotes.json"))
                .unwrap();
        assert_eq!(
            raw["sdk"].as_str().unwrap(),
            "@meteora-ag/dynamic-bonding-curve-sdk@2.0.0"
        );
        for curve_v in raw["curves"].as_array().unwrap() {
            let curve = curve_from_value(curve_v);
            validate_curve(&curve).unwrap();
            for quote in curve_v["quotes"].as_array().unwrap() {
                let amount_in: u64 = quote["amountIn"].as_str().unwrap().parse().unwrap();
                let expected: u64 = quote["outputAmount"].as_str().unwrap().parse().unwrap();
                let got = quote_first_buy(&curve, amount_in).unwrap();
                assert_eq!(
                    got,
                    expected,
                    "{} amount_in {amount_in}",
                    curve_v["name"].as_str().unwrap()
                );
            }
        }
    }

    #[test]
    fn live_m9_over_report_would_strand_a_later_60_percent_claimer() {
        // trust3: watcher reported 1.5×. c5 (30%) took 1_400_119_995_884,
        // which is 466_706_665_295 over a fair 30% of the real raise.
        let claimed = 1_400_119_995_884u128;
        let over = 466_706_665_295u128;
        let fair30 = claimed - over;
        assert_eq!(fair30, 933_413_330_589);
        let real = fair30 * 10 / 3;
        assert_eq!(real, 3_111_377_768_630);
        let left = real - claimed;
        assert_eq!(left, 1_711_257_772_746);
        let fair60 = real * 60 / 100;
        assert!(left < fair60);
        let fair40 = real * 40 / 100;
        assert!(left > fair40);
    }

    #[test]
    fn exponential_scheduler_is_rejected() {
        let mut curve = demo_curve();
        curve.base_fee_mode = 1;
        assert_eq!(
            validate_curve(&curve).unwrap_err(),
            "only a linear fee scheduler is quoted"
        );
    }
}
