//! invariant calculator.

use anchor_lang::{require, Result};
use spl_math::precise_number::PreciseNumber;

use crate::{
    curve::{
        calculator::{
            CurveCalculator, DynAccountSerialize, RoundDirection, SwapWithoutFeesResult,
            TradeDirection, TradingTokenResult,
        },
        math,
    },
    error::SwapError,
    state::ConstantProductCurve,
    try_math,
    utils::math::{TryCeilDiv, TryMath, TryMathRef, TryNew},
};

/// The constant product swap calculation, factored out of its class for reuse.
///
/// This is guaranteed to work for all values such that:
///  - 1 <= swap_source_amount * swap_destination_amount <= u128::MAX
///  - 1 <= source_amount <= u64::MAX
pub fn swap(
    source_amount: u128,
    pool_source_amount: u128,
    pool_destination_amount: u128,
) -> Result<SwapWithoutFeesResult> {
    let invariant = try_math!(pool_source_amount.try_mul(pool_destination_amount))?;

    let new_pool_source_amount = try_math!(pool_source_amount.try_add(source_amount))?;
    let (new_pool_destination_amount, new_pool_source_amount) =
        try_math!(invariant.try_ceil_div(new_pool_source_amount))?;

    let source_amount_swapped = try_math!(new_pool_source_amount.try_sub(pool_source_amount))?;
    let destination_amount_swapped =
        try_math!(pool_destination_amount.try_sub(new_pool_destination_amount))?;

    require!(
        source_amount_swapped > 0 && destination_amount_swapped > 0,
        SwapError::ZeroTradingTokens
    );
    Ok(SwapWithoutFeesResult {
        source_amount_swapped,
        destination_amount_swapped,
    })
}

/// Calculates the total normalized value of the curve given the liquidity
/// parameters.
///
/// The constant product implementation for this function gives the square root of
/// the Uniswap invariant.
pub fn normalized_value(
    swap_token_a_amount: u128,
    swap_token_b_amount: u128,
) -> Result<PreciseNumber> {
    let swap_token_a_amount = PreciseNumber::try_new(swap_token_a_amount)?;
    let swap_token_b_amount = PreciseNumber::try_new(swap_token_b_amount)?;
    try_math!(swap_token_a_amount
        .try_mul(&swap_token_b_amount)?
        .try_sqrt())
}

impl CurveCalculator for ConstantProductCurve {
    /// Constant product swap ensures x * y = constant
    fn swap_without_fees(
        &self,
        source_amount: u128,
        pool_source_amount: u128,
        pool_destination_amount: u128,
        _trade_direction: TradeDirection,
    ) -> Result<SwapWithoutFeesResult> {
        swap(source_amount, pool_source_amount, pool_destination_amount)
    }

    /// The constant product implementation is a simple ratio calculation for how many
    /// trading tokens correspond to a certain number of pool tokens
    fn pool_tokens_to_trading_tokens(
        &self,
        pool_tokens: u128,
        pool_token_supply: u128,
        swap_token_a_amount: u128,
        swap_token_b_amount: u128,
        round_direction: RoundDirection,
    ) -> Result<TradingTokenResult> {
        math::pool_tokens_to_trading_tokens(
            pool_tokens,
            pool_token_supply,
            swap_token_a_amount,
            swap_token_b_amount,
            round_direction,
        )
    }

    fn validate(&self) -> Result<()> {
        Ok(())
    }

    fn normalized_value(
        &self,
        swap_token_a_amount: u128,
        swap_token_b_amount: u128,
    ) -> Result<PreciseNumber> {
        normalized_value(swap_token_a_amount, swap_token_b_amount)
    }
}

impl DynAccountSerialize for ConstantProductCurve {
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
    use proptest::prelude::*;

    use super::*;
    use crate::{
        curve::calculator::{
            test::{
                check_curve_value_from_swap, check_pool_value_from_deposit,
                check_pool_value_from_withdraw, total_and_intermediate,
            },
            RoundDirection, INITIAL_SWAP_POOL_AMOUNT,
        },
        state::Curve,
    };

    #[test]
    fn initial_pool_amount() {
        let calculator = ConstantProductCurve {
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
        let calculator = ConstantProductCurve {
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
    fn fail_trading_token_conversion() {
        let calculator = ConstantProductCurve {
            ..Default::default()
        };
        let results =
            calculator.pool_tokens_to_trading_tokens(5, 10, u128::MAX, 0, RoundDirection::Floor);
        assert_eq!(results, Err(SwapError::CalculationFailure.into()));
        let results =
            calculator.pool_tokens_to_trading_tokens(5, 10, 0, u128::MAX, RoundDirection::Floor);
        assert_eq!(results, Err(SwapError::CalculationFailure.into()));
    }

    #[test]
    fn serialize_constant_product_curve() {
        let curve = ConstantProductCurve {
            ..Default::default()
        };

        let mut arr = [0u8; Curve::LEN];
        let packed = arr.borrow_mut();
        let ref_mut = std::cell::RefCell::new(packed);

        curve.try_dyn_serialize(ref_mut.borrow_mut()).unwrap();
        let unpacked = ConstantProductCurve::try_deserialize(&mut arr.as_ref()).unwrap();
        assert_eq!(curve, unpacked);
    }

    fn test_truncation(
        curve: &ConstantProductCurve,
        source_amount: u128,
        swap_source_amount: u128,
        swap_destination_amount: u128,
        expected_source_amount_swapped: u128,
        expected_destination_amount_swapped: u128,
    ) {
        let invariant = swap_source_amount * swap_destination_amount;
        let result = curve
            .swap_without_fees(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
            )
            .unwrap();
        assert_eq!(result.source_amount_swapped, expected_source_amount_swapped);
        assert_eq!(
            result.destination_amount_swapped,
            expected_destination_amount_swapped
        );
        let new_invariant = (swap_source_amount + result.source_amount_swapped)
            * (swap_destination_amount - result.destination_amount_swapped);
        assert!(new_invariant >= invariant);
    }

    #[test]
    fn constant_product_swap_rounding() {
        let curve = ConstantProductCurve::default();

        // much too small
        assert!(curve
            .swap_without_fees(10, 70_000_000_000, 4_000_000, TradeDirection::AtoB)
            .is_err()); // spot: 10 * 4m / 70b = 0

        let tests: &[(u128, u128, u128, u128, u128)] = &[
            (10, 4_000_000, 70_000_000_000, 10, 174_999), // spot: 10 * 70b / ~4m = 174,999.99
            (20, 30_000 - 20, 10_000, 18, 6), // spot: 20 * 1 / 3.000 = 6.6667 (source can be 18 to get 6 dest.)
            (19, 30_000 - 20, 10_000, 18, 6), // spot: 19 * 1 / 2.999 = 6.3334 (source can be 18 to get 6 dest.)
            (18, 30_000 - 20, 10_000, 18, 6), // spot: 18 * 1 / 2.999 = 6.0001
            (10, 20_000, 30_000, 10, 14),     // spot: 10 * 3 / 2.0010 = 14.99
            (10, 20_000 - 9, 30_000, 10, 14), // spot: 10 * 3 / 2.0001 = 14.999
            (10, 20_000 - 10, 30_000, 10, 15), // spot: 10 * 3 / 2.0000 = 15
            (100, 60_000, 30_000, 99, 49), // spot: 100 * 3 / 6.001 = 49.99 (source can be 99 to get 49 dest.)
            (99, 60_000, 30_000, 99, 49),  // spot: 99 * 3 / 6.001 = 49.49
            (98, 60_000, 30_000, 97, 48), // spot: 98 * 3 / 6.001 = 48.99 (source can be 97 to get 48 dest.)
        ];
        for (
            source_amount,
            swap_source_amount,
            swap_destination_amount,
            expected_source_amount,
            expected_destination_amount,
        ) in tests.iter()
        {
            test_truncation(
                &curve,
                *source_amount,
                *swap_source_amount,
                *swap_destination_amount,
                *expected_source_amount,
                *expected_destination_amount,
            );
        }
    }

    proptest! {
        #[test]
        fn curve_value_does_not_decrease_from_swap(
            source_token_amount in 1..u64::MAX,
            swap_source_amount in 1..u64::MAX,
            swap_destination_amount in 1..u64::MAX,
        ) {
            let curve = ConstantProductCurve { ..Default::default() };
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
        fn curve_value_does_not_decrease_from_deposit(
            pool_token_amount in 1..u64::MAX,
            pool_token_supply in 1..u64::MAX,
            swap_token_a_amount in 1..u64::MAX,
            swap_token_b_amount in 1..u64::MAX,
        ) {
            let pool_token_amount = pool_token_amount as u128;
            let pool_token_supply = pool_token_supply as u128;
            let swap_token_a_amount = swap_token_a_amount as u128;
            let swap_token_b_amount = swap_token_b_amount as u128;
            // Make sure we will get at least one trading token out for each
            // side, otherwise the calculation fails
            prop_assume!(pool_token_amount * swap_token_a_amount / pool_token_supply >= 1);
            prop_assume!(pool_token_amount * swap_token_b_amount / pool_token_supply >= 1);
            let curve = ConstantProductCurve { ..Default::default() };
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
        ) {
            let pool_token_amount = pool_token_amount as u128;
            let pool_token_supply = pool_token_supply as u128;
            let swap_token_a_amount = swap_token_a_amount as u128;
            let swap_token_b_amount = swap_token_b_amount as u128;
            // Make sure we will get at least one trading token out for each
            // side, otherwise the calculation fails
            prop_assume!(pool_token_amount * swap_token_a_amount / pool_token_supply >= 1);
            prop_assume!(pool_token_amount * swap_token_b_amount / pool_token_supply >= 1);
            let curve = ConstantProductCurve { ..Default::default() };
            check_pool_value_from_withdraw(
                &curve,
                pool_token_amount,
                pool_token_supply,
                swap_token_a_amount,
                swap_token_b_amount,
            );
        }
    }
}
