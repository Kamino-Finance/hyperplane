//! Invariant calculator with an extra offset

use anchor_lang::Result;
use spl_math::precise_number::PreciseNumber;

use crate::{
    curve::{
        calculator::{
            CurveCalculator, DynAccountSerialize, RoundDirection, SwapWithoutFeesResult,
            TradeDirection, TradingTokenResult,
        },
        constant_product::{
            normalized_value, pool_tokens_to_trading_tokens, swap,
            withdraw_single_token_type_exact_out,
        },
    },
    error::SwapError,
    require_msg,
    state::OffsetCurve,
    try_math,
    utils::math::TryMath,
};

/// Offset curve, uses ConstantProduct under the hood, but adds an offset to
/// one side on swap calculations
impl CurveCalculator for OffsetCurve {
    /// Constant product swap ensures token a * (token b + offset) = constant
    /// This is guaranteed to work for all values such that:
    ///  - 1 <= source_amount <= u64::MAX
    ///  - 1 <= (swap_source_amount * (swap_destination_amount + token_b_offset)) <= u128::MAX
    /// If the offset and token B are both close to u64::MAX, there can be
    /// overflow errors with the invariant.
    fn swap_without_fees(
        &self,
        source_amount: u128,
        swap_source_amount: u128,
        swap_destination_amount: u128,
        trade_direction: TradeDirection,
    ) -> Result<SwapWithoutFeesResult> {
        let token_b_offset = self.token_b_offset as u128;
        let swap_source_amount = match trade_direction {
            TradeDirection::AtoB => swap_source_amount,
            TradeDirection::BtoA => try_math!(swap_source_amount.try_add(token_b_offset))?,
        };
        let swap_destination_amount = match trade_direction {
            TradeDirection::AtoB => try_math!(swap_destination_amount.try_add(token_b_offset))?,
            TradeDirection::BtoA => swap_destination_amount,
        };
        swap(source_amount, swap_source_amount, swap_destination_amount)
    }

    /// The conversion for the offset curve needs to take into account the
    /// offset
    fn pool_tokens_to_trading_tokens(
        &self,
        pool_tokens: u128,
        pool_token_supply: u128,
        swap_token_a_amount: u128,
        swap_token_b_amount: u128,
        round_direction: RoundDirection,
    ) -> Result<TradingTokenResult> {
        let token_b_offset = self.token_b_offset as u128;
        pool_tokens_to_trading_tokens(
            pool_tokens,
            pool_token_supply,
            swap_token_a_amount,
            swap_token_b_amount.try_add(token_b_offset)?,
            round_direction,
        )
    }

    fn withdraw_single_token_type_exact_out(
        &self,
        source_amount: u128,
        swap_token_a_amount: u128,
        swap_token_b_amount: u128,
        pool_supply: u128,
        trade_direction: TradeDirection,
        round_direction: RoundDirection,
    ) -> Result<u128> {
        let token_b_offset = u128::from(self.token_b_offset);
        withdraw_single_token_type_exact_out(
            source_amount,
            swap_token_a_amount,
            swap_token_b_amount.try_add(token_b_offset)?,
            pool_supply,
            trade_direction,
            round_direction,
        )
    }

    fn validate(&self) -> Result<()> {
        require_msg!(
            self.token_b_offset > 0,
            SwapError::InvalidCurve,
            "Token B offset must be greater than 0 for offset curve"
        );
        Ok(())
    }

    fn validate_supply(&self, token_a_amount: u64, _token_b_amount: u64) -> Result<()> {
        require_msg!(
            token_a_amount > 0,
            SwapError::EmptySupply,
            "Token A amount must be greater than 0 for offset curve"
        );
        Ok(())
    }

    /// Offset curves can cause arbitrage opportunities if outside users are
    /// allowed to deposit.  For example, in the offset curve, if there's swap
    /// with 1 million of token A against an offset of 2 million token B,
    /// someone else can deposit 1 million A and 2 million B for LP tokens.
    /// The pool creator can then use their LP tokens to steal the 2 million B,
    fn allows_deposits(&self) -> bool {
        false
    }

    /// The normalized value of the offset curve simply needs to add the offset to
    /// the token B side before calculating
    fn normalized_value(
        &self,
        swap_token_a_amount: u128,
        swap_token_b_amount: u128,
    ) -> Result<PreciseNumber> {
        let token_b_offset = self.token_b_offset as u128;
        normalized_value(
            swap_token_a_amount,
            try_math!(swap_token_b_amount.try_add(token_b_offset))?,
        )
    }
}

impl DynAccountSerialize for OffsetCurve {
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
        curve::calculator::test::{
            check_curve_value_from_swap, check_pool_value_from_deposit,
            check_pool_value_from_withdraw, check_withdraw_token_conversion,
            total_and_intermediate, CONVERSION_BASIS_POINTS_GUARANTEE,
        },
        state::Curve,
    };

    #[test]
    fn serialize_offset_curve() {
        let token_b_offset = u64::MAX;
        let curve = OffsetCurve {
            token_b_offset,
            ..Default::default()
        };

        let mut arr = [0u8; Curve::LEN];
        let packed = arr.borrow_mut();
        let ref_mut = std::cell::RefCell::new(packed);

        curve.try_dyn_serialize(ref_mut.borrow_mut()).unwrap();
        let unpacked = OffsetCurve::try_deserialize(&mut arr.as_ref()).unwrap();
        assert_eq!(curve, unpacked);
    }

    #[test]
    fn swap_no_offset() {
        let swap_source_amount: u128 = 1_000;
        let swap_destination_amount: u128 = 50_000;
        let source_amount: u128 = 100;
        let curve = OffsetCurve::default();
        let result = curve
            .swap_without_fees(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
            )
            .unwrap();
        assert_eq!(result.source_amount_swapped, source_amount);
        assert_eq!(result.destination_amount_swapped, 4545);
        let result = curve
            .swap_without_fees(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::BtoA,
            )
            .unwrap();
        assert_eq!(result.source_amount_swapped, source_amount);
        assert_eq!(result.destination_amount_swapped, 4545);
    }

    #[test]
    fn swap_offset() {
        let swap_source_amount: u128 = 1_000_000;
        let swap_destination_amount: u128 = 0;
        let source_amount: u128 = 100;
        let token_b_offset = 1_000_000;
        let curve = OffsetCurve {
            token_b_offset,
            ..Default::default()
        };
        let result = curve
            .swap_without_fees(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
            )
            .unwrap();
        assert_eq!(result.source_amount_swapped, source_amount);
        assert_eq!(result.destination_amount_swapped, source_amount - 1);

        let bad_result = curve.swap_without_fees(
            source_amount,
            swap_source_amount,
            swap_destination_amount,
            TradeDirection::BtoA,
        );
        assert!(bad_result.is_err());
    }

    #[test]
    fn swap_a_to_b_max_offset() {
        let swap_source_amount: u128 = 10_000_000;
        let swap_destination_amount: u128 = 1_000;
        let source_amount: u128 = 1_000;
        let token_b_offset = u64::MAX;
        let curve = OffsetCurve {
            token_b_offset,
            ..Default::default()
        };
        let result = curve
            .swap_without_fees(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
            )
            .unwrap();
        assert_eq!(result.source_amount_swapped, source_amount);
        assert_eq!(result.destination_amount_swapped, 1_844_489_958_375_117);
    }

    #[test]
    fn swap_b_to_a_max_offset() {
        let swap_source_amount: u128 = 10_000_000;
        let swap_destination_amount: u128 = 1_000;
        let source_amount: u128 = u64::MAX.into();
        let token_b_offset = u64::MAX;
        let curve = OffsetCurve {
            token_b_offset,
            ..Default::default()
        };
        let result = curve
            .swap_without_fees(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::BtoA,
            )
            .unwrap();
        assert_eq!(result.source_amount_swapped, 18_373_104_376_818_475_561);
        assert_eq!(result.destination_amount_swapped, 499);
    }

    prop_compose! {
        pub fn values_sum_within_u64()(total in 1..u64::MAX)
                        (amount in 1..total, total in Just(total))
                        -> (u64, u64) {
           (total - amount, amount)
       }
    }

    proptest! {
        #[test]
        fn withdraw_token_conversion(
            (pool_token_supply, pool_token_amount) in total_and_intermediate(u64::MAX),
            swap_token_a_amount in 1..u64::MAX,
            (swap_token_b_amount, token_b_offset) in values_sum_within_u64(),
        ) {
            let curve = OffsetCurve {
                token_b_offset,
                ..Default::default()
            };

            let swap_token_a_amount = swap_token_a_amount as u128;
            let swap_token_b_amount = swap_token_b_amount as u128;
            let token_b_offset = token_b_offset as u128;
            let pool_token_amount = pool_token_amount as u128;
            let pool_token_supply = pool_token_supply as u128;
            // The invariant needs to fit in a u128
            // invariant = swap_destination_amount * (swap_source_amount + token_b_offset)
            prop_assume!(!(swap_token_b_amount + token_b_offset).overflowing_mul(swap_token_a_amount).1);
            prop_assume!(pool_token_amount * swap_token_a_amount / pool_token_supply >= 1);
            prop_assume!(pool_token_amount * (swap_token_b_amount + token_b_offset) / pool_token_supply >= 1);
            // make sure we don't overdraw from either side
            let withdraw_result = curve
                .pool_tokens_to_trading_tokens(
                    pool_token_amount,
                    pool_token_supply,
                    swap_token_a_amount,
                    swap_token_b_amount,
                    RoundDirection::Floor,
                )
                .unwrap();
            prop_assume!(withdraw_result.token_b_amount <= swap_token_b_amount); // avoid overdrawing to 0 for calc
            check_withdraw_token_conversion(
                &curve,
                pool_token_amount,
                pool_token_supply,
                swap_token_a_amount,
                swap_token_b_amount,
                TradeDirection::AtoB,
                CONVERSION_BASIS_POINTS_GUARANTEE
            );
            check_withdraw_token_conversion(
                &curve,
                pool_token_amount,
                pool_token_supply,
                swap_token_a_amount,
                swap_token_b_amount,
                TradeDirection::BtoA,
                CONVERSION_BASIS_POINTS_GUARANTEE
            );
        }
    }

    proptest! {
        #[test]
        fn curve_value_does_not_decrease_from_swap_a_to_b(
            source_token_amount in 1..u64::MAX,
            swap_source_amount in 1..u64::MAX,
            swap_destination_amount in 1..u64::MAX,
            token_b_offset in 1..u64::MAX,
        ) {
            let curve = OffsetCurve { token_b_offset, ..Default::default() };

            let source_token_amount = source_token_amount as u128;
            let swap_source_amount = swap_source_amount as u128;
            let swap_destination_amount = swap_destination_amount as u128;
            let token_b_offset = token_b_offset as u128;

            // The invariant needs to fit in a u128
            // invariant = swap_source_amount * (swap_destination_amount + token_b_offset)
            prop_assume!(!(swap_destination_amount + token_b_offset).overflowing_mul(swap_source_amount).1);

            // In order for the swap to succeed, we need to make
            // sure that we don't overdraw on the token B side, ie.
            // (B + offset) - (B + offset) * A / (A + A_in) <= B
            // which reduces to
            // A_in * offset <= A * B
            prop_assume!(
                (source_token_amount * token_b_offset) <=
                (swap_source_amount * swap_destination_amount));
            check_curve_value_from_swap(
                &curve,
                source_token_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB
            );
        }
    }

    proptest! {
        #[test]
        fn curve_value_does_not_decrease_from_swap_b_to_a(
            source_token_amount in 1..u64::MAX,
            swap_source_amount in 1..u64::MAX,
            swap_destination_amount in 1..u64::MAX,
            token_b_offset in 1..u64::MAX,
        ) {
            let curve = OffsetCurve { token_b_offset, ..Default::default() };

            let source_token_amount = source_token_amount as u128;
            let swap_source_amount = swap_source_amount as u128;
            let swap_destination_amount = swap_destination_amount as u128;
            let token_b_offset = token_b_offset as u128;

            // The invariant needs to fit in a u128
            // invariant = swap_destination_amount * (swap_source_amount + token_b_offset)
            prop_assume!(!(swap_source_amount + token_b_offset).overflowing_mul(swap_destination_amount).1);
            check_curve_value_from_swap(
                &curve,
                source_token_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::BtoA
            );
        }
    }

    proptest! {
        #[test]
        fn curve_value_does_not_decrease_from_deposit(
            pool_token_amount in 1..u64::MAX,
            pool_token_supply in 1..u64::MAX,
            swap_token_a_amount in 1..u64::MAX,
            (swap_token_b_amount, token_b_offset) in values_sum_within_u64(),
        ) {
            let curve = OffsetCurve { token_b_offset, ..Default::default() };
            let pool_token_amount = pool_token_amount as u128;
            let pool_token_supply = pool_token_supply as u128;
            let swap_token_a_amount = swap_token_a_amount as u128;
            let swap_token_b_amount = swap_token_b_amount as u128;
            let token_b_offset = token_b_offset as u128;

            // Make sure we will get at least one trading token out for each
            // side, otherwise the calculation fails
            prop_assume!(pool_token_amount * swap_token_a_amount / pool_token_supply >= 1);
            prop_assume!(pool_token_amount * (swap_token_b_amount + token_b_offset) / pool_token_supply >= 1);
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
            (swap_token_b_amount, token_b_offset) in values_sum_within_u64(),
        ) {
            let curve = OffsetCurve { token_b_offset, ..Default::default() };
            let pool_token_amount = pool_token_amount as u128;
            let pool_token_supply = pool_token_supply as u128;
            let swap_token_a_amount = swap_token_a_amount as u128;
            let swap_token_b_amount = swap_token_b_amount as u128;
            let token_b_offset = token_b_offset as u128;

            // Make sure we will get at least one trading token out for each
            // side, otherwise the calculation fails
            prop_assume!(pool_token_amount * swap_token_a_amount / pool_token_supply >= 1);
            prop_assume!(pool_token_amount * (swap_token_b_amount + token_b_offset) / pool_token_supply >= 1);

            // make sure we don't overdraw from either side
            let withdraw_result = curve
                .pool_tokens_to_trading_tokens(
                    pool_token_amount,
                    pool_token_supply,
                    swap_token_a_amount,
                    swap_token_b_amount,
                    RoundDirection::Floor,
                )
                .unwrap();
            prop_assume!(withdraw_result.token_b_amount <= swap_token_b_amount); // avoid overdrawing to 0 for calc

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
