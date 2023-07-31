//! Base curve implementation

use std::{fmt::Debug, sync::Arc};

use anchor_lang::{error, Result};
use anchor_lang::solana_program::clock::Epoch;
use anchor_spl::token_interface::spl_token_2022::extension::transfer_fee::TransferFeeConfig;
#[cfg(feature = "fuzz")]
use arbitrary::Arbitrary;
use derive_more::Constructor;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{curve::{
    calculator::{CurveCalculator, SwapWithoutFeesResult, TradeDirection},
    fees::Fees,
}, model::CurveParameters, state::{ConstantPriceCurve, ConstantProductCurve, OffsetCurve, StableCurve}, to_u64, try_math, utils::math::TryMath};
use crate::error::SwapError;

/// Curve types supported by the hyperplane program.
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum CurveType {
    /// Uniswap-style constant product curve, invariant = token_a_amount * token_b_amount
    ConstantProduct = 1,
    /// Flat line, always providing 1:1 from one token to another
    ConstantPrice = 2,
    /// Offset curve, like Uniswap, but the token B side has a faked offset
    Offset = 3,
    /// Stable curve, like constant product with less slippage around a fixed price
    Stable = 4,
}

/// Encodes all fee inputs which should be considered when swapping
#[derive(Debug, PartialEq, Constructor)]
pub struct SwapFeeInputs<'swap_fees, 'xfer_fees> {
    /// The pool swap fees config
    pub pool_fees: &'swap_fees Fees,
    /// Optional token 2022 mint transfer fees config
    pub transfer_fees: Option<(&'xfer_fees TransferFeeConfig, Epoch)>,
    /// Flag indicating whether host fees should be deducted from owner fees. i.e. the caller passed a token account to the ix in order to collect host fees
    pub host_fees: bool,
}

impl<'pool_fees, 'xfer_fees> SwapFeeInputs<'pool_fees, 'xfer_fees> {
    pub fn pool_fees(pool_fees: &'pool_fees Fees) -> SwapFeeInputs<'pool_fees, 'xfer_fees> {
        SwapFeeInputs {
            pool_fees,
            transfer_fees: None,
            host_fees: false,
        }
    }
}

/// Encodes all results of swapping from a source token to a destination token
#[derive(Debug, PartialEq)]
pub struct SwapResult {
    /// New amount of source token in the pool vaults
    pub new_pool_source_amount: u128,
    /// New amount of destination token in the pool vaults
    pub new_pool_destination_amount: u128,
    /// Total amount of source tokens debited from user (includes: admin, trading, + host fees)
    pub source_amount_swapped: u128,
    /// Amount of destination token swapped and sent to user
    pub destination_amount_swapped: u128,
    /// Amount of source token to transfer to the vault (trade_fee + source_amount_swapped)
    pub source_amount_to_vault: u128,
    /// Amount of source tokens going to pool holders
    pub trade_fee: u128,
    /// Amount of source tokens going to owner
    pub owner_fee: u128,
    /// Amount of source tokens going to host
    pub host_fee: u128,
}

impl SwapResult {
    pub fn total_fees(&self) -> Result<u128> {
        try_math!(try_math!(try_math!(self.trade_fee.try_add(self.owner_fee))? .try_add(self.host_fee)))
    }
}

/// Concrete struct to wrap around the trait object which performs calculation.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SwapCurve {
    /// The type of curve contained in the calculator, helpful for outside
    /// queries
    pub curve_type: CurveType,
    /// The actual calculator, represented as a trait object to allow for many
    /// different types of curves
    pub calculator: Arc<dyn CurveCalculator + Sync + Send>,
}

impl SwapCurve {
    pub fn new_from_params(curve_params: CurveParameters) -> Result<Self> {
        let curve = match curve_params {
            CurveParameters::ConstantProduct => SwapCurve {
                curve_type: CurveType::ConstantProduct,
                calculator: Arc::new(ConstantProductCurve {
                    ..Default::default()
                }),
            },
            CurveParameters::ConstantPrice { token_b_price } => SwapCurve {
                curve_type: CurveType::ConstantPrice,
                calculator: Arc::new(ConstantPriceCurve {
                    token_b_price,
                    ..Default::default()
                }),
            },
            CurveParameters::Offset { token_b_offset } => SwapCurve {
                curve_type: CurveType::Offset,
                calculator: Arc::new(OffsetCurve {
                    token_b_offset,
                    ..Default::default()
                }),
            },
            CurveParameters::Stable {
                amp,
                token_a_decimals,
                token_b_decimals,
            } => SwapCurve {
                curve_type: CurveType::Stable,
                calculator: Arc::new(StableCurve::new(amp, token_a_decimals, token_b_decimals)?),
            },
        };
        Ok(curve)
    }

    /// Subtract fees and calculate how much destination token will be provided
    /// given an amount of source token.
    pub fn swap(
        &self,
        source_amount: u128,
        pool_source_amt: u128,
        pool_destination_amt: u128,
        trade_direction: TradeDirection,
        fees: &SwapFeeInputs,
    ) -> Result<SwapResult> {
        let pool_fees = fees.pool_fees;
        // debit the fee to calculate the amount swapped
        let owner_and_host_fee = try_math!(pool_fees.owner_trading_fee(source_amount))?;
        let host_fee = if fees.host_fees {
            try_math!(pool_fees.host_fee(owner_and_host_fee))?
        } else {
            0
        };
        let owner_fee = try_math!(owner_and_host_fee.try_sub(host_fee))?;
        let source_amt_sub_owner_fees = try_math!(source_amount.try_sub(owner_and_host_fee))?;

        let source_amt_sub_xfer_fees = match fees.transfer_fees {
            None => source_amt_sub_owner_fees,
            Some((xfer_fee_config, epoch)) => {
                let xfer_fee = xfer_fee_config.calculate_epoch_fee(epoch, to_u64!(source_amt_sub_owner_fees)?)
                    .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
                try_math!(source_amt_sub_owner_fees.try_sub(xfer_fee.into()))?
            }
        };

        let trade_fee = try_math!(pool_fees.trading_fee(source_amt_sub_xfer_fees))?;
        let source_amt_sub_fees = try_math!(source_amt_sub_xfer_fees.try_sub(trade_fee))?;

        let SwapWithoutFeesResult {
            source_amount_swapped,
            destination_amount_swapped,
        } = self.calculator.swap_without_fees(
            source_amt_sub_fees,
            pool_source_amt,
            pool_destination_amt,
            trade_direction,
        )?;

        let source_amt_before_xfer_fees = try_math!(source_amount_swapped.try_add(trade_fee))?;

        let source_amount_to_vault = match fees.transfer_fees {
            None => source_amt_before_xfer_fees,
            Some((xfer_fee_config, epoch)) => {
                let transfer_fee = xfer_fee_config
                    .calculate_inverse_epoch_fee(epoch, to_u64!(source_amt_before_xfer_fees)?)
                    .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
                try_math!(source_amt_before_xfer_fees.try_add(transfer_fee.into()))?
            }
        };

        Ok(SwapResult {
            new_pool_source_amount: try_math!(pool_source_amt.try_add(source_amount_to_vault))?,
            new_pool_destination_amount: try_math!(
                pool_destination_amt.try_sub(destination_amount_swapped)
            )?,
            source_amount_swapped,
            destination_amount_swapped,
            source_amount_to_vault,
            trade_fee,
            owner_fee,
            host_fee,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn constant_product_trade_fee() {
        // calculation on https://github.com/solana-labs/solana-program-library/issues/341
        let swap_source_amount = 1000;
        let swap_destination_amount = 50000;
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 100;
        let owner_trade_fee_numerator = 0;
        let owner_trade_fee_denominator = 0;
        let owner_withdraw_fee_numerator = 0;
        let owner_withdraw_fee_denominator = 0;
        let host_fee_numerator = 0;
        let host_fee_denominator = 0;

        let pool_fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };
        let swap_fee_inputs = SwapFeeInputs::new(
            &pool_fees,
            None,
            false,
        );
        let source_amount = 100;
        let curve = ConstantProductCurve {
            ..Default::default()
        };
        let swap_curve = SwapCurve {
            curve_type: CurveType::ConstantProduct,
            calculator: Arc::new(curve),
        };
        let result = swap_curve
            .swap(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
                &swap_fee_inputs,
            )
            .unwrap();
        assert_eq!(result.new_pool_source_amount, 1100);
        assert_eq!(result.destination_amount_swapped, 4504);
        assert_eq!(result.new_pool_destination_amount, 45496);
        assert_eq!(result.trade_fee, 1);
        assert_eq!(result.owner_fee, 0);
    }

    #[test]
    fn constant_product_owner_fee() {
        // calculation on https://github.com/solana-labs/solana-program-library/issues/341
        let swap_source_amount = 1000;
        let swap_destination_amount = 50000;
        let trade_fee_numerator = 0;
        let trade_fee_denominator = 0;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 100;
        let owner_withdraw_fee_numerator = 0;
        let owner_withdraw_fee_denominator = 0;
        let host_fee_numerator = 0;
        let host_fee_denominator = 0;
        let pool_fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };
        let swap_fee_inputs = SwapFeeInputs::new(
            &pool_fees,
            None,
            false,
        );
        let source_amount: u128 = 100;
        let curve = ConstantProductCurve {
            ..Default::default()
        };
        let swap_curve = SwapCurve {
            curve_type: CurveType::ConstantProduct,
            calculator: Arc::new(curve),
        };
        let result = swap_curve
            .swap(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
                &swap_fee_inputs,
            )
            .unwrap();
        // todo elliot
        assert_eq!(result.new_pool_source_amount, 1099);
        // assert_eq!(result.total_source_amount_swapped, 100);
        assert_eq!(result.source_amount_swapped, 99);
        assert_eq!(result.destination_amount_swapped, 4504);
        assert_eq!(result.new_pool_destination_amount, 45496);
        assert_eq!(result.trade_fee, 0);
        assert_eq!(result.owner_fee, 1);
    }

    #[test]
    fn constant_product_no_fee() {
        let swap_source_amount: u128 = 1_000;
        let swap_destination_amount: u128 = 50_000;
        let source_amount: u128 = 100;
        let curve = ConstantProductCurve::default();
        let pool_fees = Fees::default();
        let swap_fee_inputs = SwapFeeInputs::new(
            &pool_fees,
            None,
            false,
        );
        let swap_curve = SwapCurve {
            curve_type: CurveType::ConstantProduct,
            calculator: Arc::new(curve),
        };
        let result = swap_curve
            .swap(
                source_amount,
                swap_source_amount,
                swap_destination_amount,
                TradeDirection::AtoB,
                &swap_fee_inputs,
            )
            .unwrap();
        assert_eq!(result.new_pool_source_amount, 1100);
        assert_eq!(result.destination_amount_swapped, 4545);
        assert_eq!(result.new_pool_destination_amount, 45455);
    }
}
