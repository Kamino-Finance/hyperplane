//! The stableswap invariant calculator.
use std::convert::TryFrom;

use anchor_lang::{error, Result};
use spl_math::{precise_number::PreciseNumber, uint::U256};

use crate::{
    curve::calculator::{
        CurveCalculator, DynAccountSerialize, RoundDirection, SwapWithoutFeesResult,
        TradeDirection, TradingTokenResult,
    },
    error::SwapError,
    require_msg,
    state::StableCurve,
    try_math,
    utils::math::{AbsDiff, TryCeilDiv, TryMath, TryMathRef, TryNew},
};

const N_COINS: u8 = 2;
/// n**n
const N_COINS_SQUARED: u8 = N_COINS * N_COINS; // 4

const ITERATIONS: u16 = 256;

/// Minimum amplification coefficient.
pub const MIN_AMP: u64 = 1;

/// Maximum amplification coefficient.
pub const MAX_AMP: u64 = 1_000_000;

/// Calculates An**n for deriving D
///
/// We choose to use A * n rather than A * n**n because `D**n / prod(x)` loses precision with a huge A value.
fn compute_ann(amp: u64) -> Result<u64> {
    amp.try_mul(N_COINS as u64)
}

/// Returns self to the power of b
fn try_u8_power(a: &U256, b: u8) -> Result<U256> {
    let mut result = *a;
    for _ in 1..b {
        result = result.try_mul(*a)?;
    }
    Ok(result)
}

/// Returns self multiplied by b
fn try_u8_mul(a: &U256, b: u8) -> Result<U256> {
    let mut result = *a;
    for _ in 1..b {
        result = result.try_add(*a)?;
    }
    Ok(result)
}

/// D = (AnnS + D_P * n) * D / ((Ann - 1) * D + (n + 1) * D_P)
///
/// * `ann` - An**n - Invariant of A - the amplification coefficient times n**(n-1)
/// * `d_init` - Current approximate value of D
/// * `d_product` - Product of all the balances - prod(x/D) // todo - elliot
/// * `sum_x` - sum(x_i) - S - Sum of all the balances
fn compute_next_d(ann: u64, d_init: &U256, d_product: &U256, sum_x: u128) -> Result<U256> {
    // An**n * sum(x)
    let anns = try_math!(U256::from(ann).try_mul(sum_x.into()))?;

    // D = (AnnS + D_P * n) * D / ((Ann - 1) * D + (n + 1) * D_P)
    let numerator = try_math!(anns
        .try_add(try_u8_mul(d_product, N_COINS)?)?
        .try_mul(*d_init))?;
    let denominator = try_math!(d_init
        .try_mul((ann.try_sub(1)?).into())?
        .try_add(try_u8_mul(d_product, N_COINS.try_add(1)?)?))?;

    try_math!(numerator.try_div(denominator))
}

/// Compute stable swap invariant (D)
///
/// Defined as:
///
/// ```md
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
/// ```
/// The value of D is calculated by solving the above equation for D.
///
/// Given all other parameters are constant, f(D), a polynomial function of degree n+1, is represented as:
///
/// ```md
/// f(D) = D**n+1 / n**n * prod(x_i) + (Ann - 1)D - AnnS = 0
///
/// The derivative of the above function is:
///
/// f'(D) = (n + 1) * D_P / D + (Ann - 1)
///
/// Where:
/// - S = sum(x_i)
/// - D_P = D**(n+1) / n**n * prod(x_i)
/// ```
///
/// Solve for D using [Newton's method](https://en.wikipedia.org/wiki/Newton%27s_method):
/// ```md
/// Newton's method:
/// x_n+1 = x_n - f(x_n) / f'(x_n)
///
/// Therefore:
/// D_n+1 = D_n - f(D_n) / f'(D_n)
///
/// Iteratively solve for D:
///
/// D = (AnnS + D_P * n) * D / ((Ann - 1) * D + (n + 1) * D_P)
/// ```
///
/// * `ann` - The invariant of A - the amplification coefficient times n**(n-1)
/// * `amount_a` - The number of A tokens in the pool
/// * `amount_b` - The number of B tokens in the pool
fn compute_d(ann: u64, amount_a: u128, amount_b: u128) -> Result<u128> {
    let sum_x = try_math!(amount_a.try_add(amount_b))?; // sum(x_i), a.k.a S
    if sum_x == 0 {
        Ok(0)
    } else {
        let amount_a_times_coins = try_u8_mul(&U256::from(amount_a), N_COINS)?;
        let amount_b_times_coins = try_u8_mul(&U256::from(amount_b), N_COINS)?;

        let mut d_previous: U256;
        // start by guessing D with the sum(x_i)
        let mut d: U256 = sum_x.into();

        // Iteratively approximate D
        for _ in 0..ITERATIONS {
            // D_P = D**(n+1) / n**n * prod(x_i)
            let mut d_product = d;
            d_product = try_math!(d_product.try_mul(d)?.try_div(amount_a_times_coins))?;
            d_product = try_math!(d_product.try_mul(d)?.try_div(amount_b_times_coins))?;
            d_previous = d;
            // D = (AnnS + D_P * n) * D / ((Ann - 1) * D + (n + 1) * D_P)
            d = compute_next_d(ann, &d, &d_product, sum_x)?;

            // Equality with the precision of 1
            if d.abs_diff(d_previous) <= 1.into() {
                break;
            }
        }
        u128::try_from(d).map_err(|_| error!(SwapError::ConversionFailure))
    }
}

/// Compute swap amount `y` in proportion to `x`
///
/// Solve the quadratic equation iteratively for y:
///
/// ```md
///
/// A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))
///
/// This forms a polynomial of degree 2 in y:
///
/// f(y) = y**2 + (b - D)y - c = 0
///
/// Where:
/// - b = S + D / Ann
/// - c = D**n+1 / n**n * P * Ann
/// - S = sum(x_i) where i != j
/// - P = prod(x_i) where i != j
///
/// To find the root:
///
/// y_n+1 = y_n - y_n**2 + (b - D)y - c / 2y_n + b - D = y_n**2 + c / 2y_n + b - D
///
/// This can be calculated using Newton's method by iterating:
///
/// y = y**2 + c / 2y + b - D
///
/// The initial value of y is D
/// ```
///
/// * `ann` - A * n**n - Ann - The invariant of A - the amplification coefficient times n**(n-1)
/// * `x` - The number of source tokens in the pool after depositing swap amount
/// * `d` - D - The total amount of tokens when they have an equal price i.e. at equilibrium when all tokens have equal balance
fn compute_y(ann: u64, x: u128, d: u128) -> Result<u128> {
    // Upscale to U256
    let ann: U256 = ann.into();
    let new_source_amount: U256 = x.into();
    let d: U256 = d.into();
    let zero = U256::zero();
    let one = U256::one();

    // b = S + D / Ann
    let b = try_math!(new_source_amount.try_add(d.try_div(ann)?))?;

    // c = D**n+1 / n**n * P * Ann
    let c = try_math!(try_u8_power(&d, N_COINS.try_add(1)?)?
        .try_div(try_u8_mul(&new_source_amount, N_COINS_SQUARED)?.try_mul(ann)?))?;

    // Solve for y:
    let mut y = d;
    for _ in 0..ITERATIONS {
        // y = y**2 + c / 2y + b - D
        let numerator = try_math!(try_u8_power(&y, 2)?.try_add(c))?;
        let denominator = try_math!(try_u8_mul(&y, 2)?.try_add(b)?.try_sub(d))?;
        // ceil_div is conservative, not allowing for a 0 return, but we can
        // ceiling to 1 token in this case since we're solving through approximation,
        // and not doing a constant product calculation
        let (y_new, _) = numerator.try_ceil_div(denominator).unwrap_or_else(|_| {
            if numerator == U256::from(0u128) {
                (zero, zero)
            } else {
                (one, zero)
            }
        });
        if y_new == y {
            break;
        } else {
            y = y_new;
        }
    }
    u128::try_from(y).map_err(|_| error!(SwapError::CalculationFailure))
}

impl CurveCalculator for StableCurve {
    /// Stable curve
    fn swap_without_fees(
        &self,
        source_amount: u128,
        pool_source_amount: u128,
        pool_destination_amount: u128,
        _trade_direction: TradeDirection,
    ) -> Result<SwapWithoutFeesResult> {
        if source_amount == 0 {
            return Ok(SwapWithoutFeesResult {
                source_amount_swapped: 0,
                destination_amount_swapped: 0,
            });
        }
        let ann = compute_ann(self.amp)?;

        let new_source_amount = try_math!(pool_source_amount.try_add(source_amount))?;
        let new_destination_amount = compute_y(
            ann,
            new_source_amount,
            compute_d(ann, pool_source_amount, pool_destination_amount)?,
        )?;

        let amount_swapped = try_math!(pool_destination_amount.try_sub(new_destination_amount))?;

        Ok(SwapWithoutFeesResult {
            source_amount_swapped: source_amount,
            destination_amount_swapped: amount_swapped,
        })
    }

    /// Remove pool tokens from the pool in exchange for trading tokens
    fn pool_tokens_to_trading_tokens(
        &self,
        pool_tokens: u128,
        pool_token_supply: u128,
        pool_token_a_amount: u128,
        pool_token_b_amount: u128,
        round_direction: RoundDirection,
    ) -> Result<TradingTokenResult> {
        let mut token_a_amount = try_math!(pool_tokens
            .try_mul(pool_token_a_amount)?
            .try_div(pool_token_supply))?;
        let mut token_b_amount = try_math!(pool_tokens
            .try_mul(pool_token_b_amount)?
            .try_div(pool_token_supply))?;
        let (token_a_amount, token_b_amount) = match round_direction {
            RoundDirection::Floor => (token_a_amount, token_b_amount),
            RoundDirection::Ceiling => {
                let token_a_remainder = try_math!(pool_tokens
                    .try_mul(pool_token_a_amount)?
                    .try_rem(pool_token_supply))?;

                if token_a_remainder > 0 && token_a_amount > 0 {
                    token_a_amount += 1;
                }
                let token_b_remainder = try_math!(pool_tokens
                    .try_mul(pool_token_b_amount)?
                    .try_rem(pool_token_supply))?;
                if token_b_remainder > 0 && token_b_amount > 0 {
                    token_b_amount += 1;
                }
                (token_a_amount, token_b_amount)
            }
        };
        Ok(TradingTokenResult {
            token_a_amount,
            token_b_amount,
        })
    }

    /// Get the amount of pool tokens for the given amount of token A or B.
    fn deposit_single_token_type(
        &self,
        source_amount: u128,
        pool_token_a_amount: u128,
        pool_token_b_amount: u128,
        pool_supply: u128,
        trade_direction: TradeDirection,
    ) -> Result<u128> {
        if source_amount == 0 {
            return Ok(0);
        }
        let ann = compute_ann(self.amp)?;
        let d0 = PreciseNumber::try_new(compute_d(ann, pool_token_a_amount, pool_token_b_amount)?)?;
        let (deposit_token_amount, other_token_amount) = match trade_direction {
            TradeDirection::AtoB => (pool_token_a_amount, pool_token_b_amount),
            TradeDirection::BtoA => (pool_token_b_amount, pool_token_a_amount),
        };
        let updated_deposit_token_amount = try_math!(deposit_token_amount.try_add(source_amount))?;
        let d1 = PreciseNumber::try_new(compute_d(
            ann,
            updated_deposit_token_amount,
            other_token_amount,
        )?)?;
        let diff = try_math!(d1.try_sub(&d0))?;
        let final_amount =
            try_math!((diff.try_mul(&PreciseNumber::try_new(pool_supply)?))?.try_div(&d0))?;
        final_amount.try_floor()?.try_to_imprecise()
    }

    fn withdraw_single_token_type_exact_out(
        &self,
        source_amount: u128,
        pool_token_a_amount: u128,
        pool_token_b_amount: u128,
        pool_supply: u128,
        trade_direction: TradeDirection,
        round_direction: RoundDirection,
    ) -> Result<u128> {
        if source_amount == 0 {
            return Ok(0);
        }
        let ann = compute_ann(self.amp)?;
        let d0 = PreciseNumber::try_new(compute_d(ann, pool_token_a_amount, pool_token_b_amount)?)?;
        let (withdraw_token_amount, other_token_amount) = match trade_direction {
            TradeDirection::AtoB => (pool_token_a_amount, pool_token_b_amount),
            TradeDirection::BtoA => (pool_token_b_amount, pool_token_a_amount),
        };
        let updated_deposit_token_amount = try_math!(withdraw_token_amount.try_sub(source_amount))?;
        let d1 = PreciseNumber::try_new(compute_d(
            ann,
            updated_deposit_token_amount,
            other_token_amount,
        )?)?;
        let diff = try_math!(d0.try_sub(&d1))?;
        let final_amount =
            try_math!((diff.try_mul(&PreciseNumber::try_new(pool_supply)?))?.try_div(&d0))?;
        match round_direction {
            RoundDirection::Floor => final_amount.try_floor()?.try_to_imprecise(),
            RoundDirection::Ceiling => final_amount.try_ceil()?.try_to_imprecise(),
        }
    }

    fn validate(&self) -> Result<()> {
        require_msg!(
            self.amp > MIN_AMP,
            SwapError::InvalidCurve,
            &format!("amp={} <= MIN_AMP={}", self.amp, MIN_AMP)
        );
        require_msg!(
            self.amp < MAX_AMP,
            SwapError::InvalidCurve,
            &format!("amp={} >= MAX_AMP={}", self.amp, MAX_AMP)
        );

        Ok(())
    }

    fn normalized_value(
        &self,
        pool_token_a_amount: u128,
        pool_token_b_amount: u128,
    ) -> Result<PreciseNumber> {
        #[cfg(not(any(test, feature = "fuzz")))]
        {
            let leverage = compute_ann(self.amp)?;
            PreciseNumber::try_new(compute_d(
                leverage,
                pool_token_a_amount,
                pool_token_b_amount,
            )?)
        }
        #[cfg(any(test, feature = "fuzz"))]
        {
            use roots::{find_roots_cubic_normalized, Roots};
            let x = pool_token_a_amount as f64;
            let y = pool_token_b_amount as f64;
            let c = (4.0 * (self.amp as f64)) - 1.0;
            let d = 16.0 * (self.amp as f64) * x * y * (x + y);
            let roots = find_roots_cubic_normalized(0.0, c, d);
            let x0 = match roots {
                Roots::No(_) => panic!("No roots found for cubic equations"),
                Roots::One(x) => x[0],
                Roots::Two(_) => panic!("Two roots found for cubic, mathematically impossible"),
                Roots::Three(x) => x[1],
                Roots::Four(_) => panic!("Four roots found for cubic, mathematically impossible"),
            };

            let root_uint = (x0 * ((10f64).powf(11.0))).round() as u128;
            let precision = PreciseNumber::try_new(10)?.try_pow(11)?;
            let two = PreciseNumber::try_new(2)?;
            PreciseNumber::try_new(root_uint)?
                .try_div(&precision)?
                .try_div(&two)
        }
    }
}

impl DynAccountSerialize for StableCurve {
    fn try_dyn_serialize(&self, mut dst: std::cell::RefMut<&mut [u8]>) -> Result<()> {
        let dst: &mut [u8] = &mut dst;
        let mut cursor = std::io::Cursor::new(dst);
        anchor_lang::AccountSerialize::try_serialize(self, &mut cursor)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::BorrowMut;

    use anchor_lang::AccountDeserialize;
    use hyperplane_sim::StableSwapModel;
    use proptest::prelude::*;

    use super::*;
    use crate::{
        curve::calculator::{
            test::{
                check_curve_value_from_swap, check_deposit_token_conversion,
                check_pool_value_from_deposit, check_pool_value_from_withdraw,
                check_withdraw_token_conversion, total_and_intermediate,
                CONVERSION_BASIS_POINTS_GUARANTEE,
            },
            RoundDirection, INITIAL_SWAP_POOL_AMOUNT,
        },
        state::Curve,
    };

    #[test]
    fn initial_pool_amount() {
        let amp = 1;
        let calculator = StableCurve {
            amp,
            ..Default::default()
        };
        assert_eq!(calculator.new_pool_supply(), INITIAL_SWAP_POOL_AMOUNT);
    }

    fn check_pool_token_rate(
        token_a: u128,
        token_b: u128,
        deposit: u128,
        supply: u128,
        expected_a: u128,
        expected_b: u128,
    ) {
        let amp = 1;
        let calculator = StableCurve {
            amp,
            ..Default::default()
        };
        let results = calculator
            .pool_tokens_to_trading_tokens(
                deposit,
                supply,
                token_a,
                token_b,
                RoundDirection::Ceiling,
            )
            .unwrap();
        assert_eq!(results.token_a_amount, expected_a);
        assert_eq!(results.token_b_amount, expected_b);
    }

    #[test]
    fn trading_token_conversion() {
        check_pool_token_rate(2, 49, 5, 10, 1, 25);
        check_pool_token_rate(100, 202, 5, 101, 5, 10);
        check_pool_token_rate(5, 501, 2, 10, 1, 101);
    }

    #[test]
    fn swap_zero() {
        let curve = StableCurve {
            amp: 100,
            ..Default::default()
        };
        let result = curve.swap_without_fees(0, 100, 1_000_000_000_000_000, TradeDirection::AtoB);

        let result = result.unwrap();
        assert_eq!(result.source_amount_swapped, 0);
        assert_eq!(result.destination_amount_swapped, 0);
    }

    #[test]
    fn serialize_stable_curve() {
        let amp = u64::MAX;
        let curve = StableCurve {
            amp,
            ..Default::default()
        };

        let mut arr = [0u8; Curve::LEN];
        let packed = arr.borrow_mut();
        let ref_mut = std::cell::RefCell::new(packed);

        curve.try_dyn_serialize(ref_mut.borrow_mut()).unwrap();
        let unpacked = StableCurve::try_deserialize(&mut arr.as_ref()).unwrap();
        assert_eq!(curve, unpacked);
    }

    proptest! {
        #[test]
        fn curve_value_does_not_decrease_from_deposit(
            pool_token_amount in 1..u64::MAX,
            pool_token_supply in 1..u64::MAX,
            swap_token_a_amount in 1..u64::MAX,
            swap_token_b_amount in 1..u64::MAX,
            amp in 1..100,
        ) {
            let pool_token_amount = pool_token_amount as u128;
            let pool_token_supply = pool_token_supply as u128;
            let swap_token_a_amount = swap_token_a_amount as u128;
            let swap_token_b_amount = swap_token_b_amount as u128;
            // Make sure we will get at least one trading token out for each
            // side, otherwise the calculation fails
            prop_assume!(pool_token_amount * swap_token_a_amount / pool_token_supply >= 1);
            prop_assume!(pool_token_amount * swap_token_b_amount / pool_token_supply >= 1);
            let curve = StableCurve {
                amp: amp as u64,
                ..Default::default()
            };
            check_pool_value_from_deposit(
                &curve,
                pool_token_amount,
                pool_token_supply,
                swap_token_a_amount,
                swap_token_b_amount,
            );
        }
    }

    proptest! {
        #[test]
        fn curve_value_does_not_decrease_from_withdraw(
            (pool_token_supply, pool_token_amount) in total_and_intermediate(u64::MAX),
            swap_token_a_amount in 1..u64::MAX,
            swap_token_b_amount in 1..u64::MAX,
            amp in 1..100,
        ) {
            let pool_token_amount = pool_token_amount as u128;
            let pool_token_supply = pool_token_supply as u128;
            let swap_token_a_amount = swap_token_a_amount as u128;
            let swap_token_b_amount = swap_token_b_amount as u128;
            // Make sure we will get at least one trading token out for each
            // side, otherwise the calculation fails
            prop_assume!(pool_token_amount * swap_token_a_amount / pool_token_supply >= 1);
            prop_assume!(pool_token_amount * swap_token_b_amount / pool_token_supply >= 1);
            let curve = StableCurve {
                amp: amp as u64,
                ..Default::default()
            };
            check_pool_value_from_withdraw(
                &curve,
                pool_token_amount,
                pool_token_supply,
                swap_token_a_amount,
                swap_token_b_amount,
            );
        }
    }

    proptest! {
        #[test]
        fn curve_value_does_not_decrease_from_swap(
            source_token_amount in 1..u64::MAX,
            swap_source_amount in 1..u64::MAX,
            swap_destination_amount in 1..u64::MAX,
            amp in 1..100,
        ) {
            let curve = StableCurve { amp: amp as u64, ..Default::default() };
            check_curve_value_from_swap(
                &curve,
                source_token_amount as u128,
                swap_source_amount as u128,
                swap_destination_amount as u128,
                TradeDirection::AtoB
            );
        }
    }

    proptest! {
        #[test]
        fn deposit_token_conversion(
            // in the pool token conversion calcs, we simulate trading half of
            // source_token_amount, so this needs to be at least 2
            source_token_amount in 2..u64::MAX,
            swap_source_amount in 1..u64::MAX,
            swap_destination_amount in 2..u64::MAX,
            pool_supply in INITIAL_SWAP_POOL_AMOUNT..u64::MAX as u128,
            amp in 1..100u64,
        ) {
            let curve = StableCurve { amp, ..Default::default() };
            check_deposit_token_conversion(
                &curve,
                source_token_amount as u128,
                swap_source_amount as u128,
                swap_destination_amount as u128,
                TradeDirection::AtoB,
                pool_supply,
                CONVERSION_BASIS_POINTS_GUARANTEE * 100,
            );

            check_deposit_token_conversion(
                &curve,
                source_token_amount as u128,
                swap_source_amount as u128,
                swap_destination_amount as u128,
                TradeDirection::BtoA,
                pool_supply,
                CONVERSION_BASIS_POINTS_GUARANTEE * 100,
            );
        }
    }

    proptest! {
        #[test]
        fn withdraw_token_conversion(
            (pool_token_supply, pool_token_amount) in total_and_intermediate(u64::MAX),
            swap_token_a_amount in 1..u64::MAX,
            swap_token_b_amount in 1..u64::MAX,
            amp in 1..100u64,
        ) {
            let curve = StableCurve { amp, ..Default::default() };
            check_withdraw_token_conversion(
                &curve,
                pool_token_amount as u128,
                pool_token_supply as u128,
                swap_token_a_amount as u128,
                swap_token_b_amount as u128,
                TradeDirection::AtoB,
                CONVERSION_BASIS_POINTS_GUARANTEE
            );
            check_withdraw_token_conversion(
                &curve,
                pool_token_amount as u128,
                pool_token_supply as u128,
                swap_token_a_amount as u128,
                swap_token_b_amount as u128,
                TradeDirection::BtoA,
                CONVERSION_BASIS_POINTS_GUARANTEE
            );
        }
    }

    // this test comes from a failed proptest
    #[test]
    fn withdraw_token_conversion_huge_withdrawal() {
        let pool_token_supply: u64 = 12798273514859089136;
        let pool_token_amount: u64 = 12798243809352362806;
        let swap_token_a_amount: u64 = 10000000000000000000;
        let swap_token_b_amount: u64 = 6000000000000000000;
        let amp = 72;
        let curve = StableCurve {
            amp,
            ..Default::default()
        };
        check_withdraw_token_conversion(
            &curve,
            pool_token_amount as u128,
            pool_token_supply as u128,
            swap_token_a_amount as u128,
            swap_token_b_amount as u128,
            TradeDirection::AtoB,
            CONVERSION_BASIS_POINTS_GUARANTEE,
        );
    }

    #[derive(Debug, Clone, Copy)]
    struct SwapTestCase {
        amp: u64,
        source_token_amount: u64,
        pool_source_amount: u64,
        pool_destination_amount: u64,
        expected_source_amount_swapped: u64,
        expected_destination_amount_swapped: u64,
    }

    fn check_swap(
        SwapTestCase {
            amp,
            source_token_amount,
            pool_source_amount,
            pool_destination_amount,
            expected_source_amount_swapped,
            expected_destination_amount_swapped,
        }: SwapTestCase,
    ) {
        let curve = StableCurve {
            amp,
            ..Default::default()
        };
        let results = curve
            .swap_without_fees(
                source_token_amount as u128,
                pool_source_amount as u128,
                pool_destination_amount as u128,
                TradeDirection::AtoB,
            )
            .unwrap();

        assert_eq!(
            results.source_amount_swapped,
            expected_source_amount_swapped as u128
        );
        assert_eq!(
            results.destination_amount_swapped,
            expected_destination_amount_swapped as u128
        );
    }

    #[test]
    fn validate_swap_amounts() {
        {
            check_swap(SwapTestCase {
                amp: 75,
                source_token_amount: 1_000_000,
                pool_source_amount: 1_000_000,
                pool_destination_amount: 1_000_000,
                expected_source_amount_swapped: 1_000_000,
                expected_destination_amount_swapped: 924_745,
            });
        }
        {
            check_swap(SwapTestCase {
                amp: 100,
                source_token_amount: 1_000_000,
                pool_source_amount: 1_000_000,
                pool_destination_amount: 1_000_000,
                expected_source_amount_swapped: 1_000_000,
                expected_destination_amount_swapped: 934_112,
            });
        }
        {
            check_swap(SwapTestCase {
                amp: 1000,
                source_token_amount: 1_000_000,
                pool_source_amount: 1_000_000,
                pool_destination_amount: 1_000_000,
                expected_source_amount_swapped: 1_000_000,
                expected_destination_amount_swapped: 978_133,
            });
        }
        {
            check_swap(SwapTestCase {
                amp: 10_000,
                source_token_amount: 1_000_000,
                pool_source_amount: 1_000_000,
                pool_destination_amount: 1_000_000,
                expected_source_amount_swapped: 1_000_000,
                expected_destination_amount_swapped: 992_978,
            });
        }
        {
            check_swap(SwapTestCase {
                amp: 100_000,
                source_token_amount: 1_000_000,
                pool_source_amount: 1_000_000,
                pool_destination_amount: 1_000_000,
                expected_source_amount_swapped: 1_000_000,
                expected_destination_amount_swapped: 997_768,
            });
        }
        {
            check_swap(SwapTestCase {
                amp: 1_000_000,
                source_token_amount: 1_000_000,
                pool_source_amount: 1_000_000,
                pool_destination_amount: 1_000_000,
                expected_source_amount_swapped: 1_000_000,
                expected_destination_amount_swapped: 999_293,
            });
        }
        {
            check_swap(SwapTestCase {
                amp: 10_000_000,
                source_token_amount: 1_000_000,
                pool_source_amount: 1_000_000,
                pool_destination_amount: 1_000_000,
                expected_source_amount_swapped: 1_000_000,
                expected_destination_amount_swapped: 999_776,
            });
        }
        check_swap(SwapTestCase {
            amp: 100_000_000,
            source_token_amount: 1_000_000,
            pool_source_amount: 1_000_000,
            pool_destination_amount: 1_000_000,
            expected_source_amount_swapped: 1_000_000,
            expected_destination_amount_swapped: 999_929,
        });
        check_swap(SwapTestCase {
            amp: 1_000_000_000,
            source_token_amount: 1_000_000,
            pool_source_amount: 1_000_000,
            pool_destination_amount: 1_000_000,
            expected_source_amount_swapped: 1_000_000,
            expected_destination_amount_swapped: 999_977,
        });
        check_swap(SwapTestCase {
            amp: 1_000_000_000,
            source_token_amount: 10_000_000,
            pool_source_amount: 1_000_000,
            pool_destination_amount: 1_000_000,
            expected_source_amount_swapped: 10_000_000,
            expected_destination_amount_swapped: 999_999,
        });
        check_swap(SwapTestCase {
            amp: 1_000_000_000,
            source_token_amount: 100_000_000,
            pool_source_amount: 1_000_000,
            pool_destination_amount: 1_000_000,
            expected_source_amount_swapped: 100_000_000,
            expected_destination_amount_swapped: 999_999,
        });
        check_swap(SwapTestCase {
            amp: 1_000_000_000,
            source_token_amount: 10_000_000,
            pool_source_amount: 1_000_000,
            pool_destination_amount: 1,
            expected_source_amount_swapped: 10_000_000,
            expected_destination_amount_swapped: 0,
        });
        check_swap(SwapTestCase {
            amp: 1_000_000_000,
            source_token_amount: 10_000_000,
            pool_source_amount: 1_000_000,
            pool_destination_amount: 1,
            expected_source_amount_swapped: 10_000_000,
            expected_destination_amount_swapped: 0,
        });
        check_swap(SwapTestCase {
            amp: 1_000_000_000,
            source_token_amount: 1,
            pool_source_amount: 1_000_000,
            pool_destination_amount: 1,
            expected_source_amount_swapped: 1,
            expected_destination_amount_swapped: 0,
        });
    }

    proptest! {
        #[test]
        fn compare_sim_swap_no_fee(
            swap_source_amount in 100..1_000_000_000_000_000_000u128,
            swap_destination_amount in 100..1_000_000_000_000_000_000u128,
            source_amount in 100..100_000_000_000u128,
            amp in 1..150u64
        ) {
            prop_assume!(source_amount < swap_source_amount);

            let curve = StableCurve { amp, ..Default::default() };

            let mut model: StableSwapModel = StableSwapModel::new(
                curve.amp.into(),
                vec![swap_source_amount, swap_destination_amount],
                N_COINS,
            );

            let result = curve.swap_without_fees(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
            );

            let result = result.unwrap();
            let sim_result = model.sim_exchange(0, 1, source_amount);

            let diff = sim_result.abs_diff(result.destination_amount_swapped);

            // tolerate a difference of 2 because of the ceiling during calculation
            let tolerance = std::cmp::max(2, sim_result / 1_000_000_000);

            assert!(
                diff <= tolerance,
                "result={}, sim_result={}, diff={}, amp={}, source_amount={}, swap_source_amount={}, swap_destination_amount={}",
                result.destination_amount_swapped,
                sim_result,
                diff,
                amp,
                source_amount,
                swap_source_amount,
                swap_destination_amount,
            );
        }
    }
}
