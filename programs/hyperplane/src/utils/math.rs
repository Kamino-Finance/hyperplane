use crate::error::SwapError;
use anchor_lang::prelude::msg;
use anchor_lang::{error, Result};
use spl_math::uint::U256;

pub trait TryMath
where
    Self: Sized,
{
    fn try_add(self, rhs: Self) -> Result<Self>;
    fn try_sub(self, rhs: Self) -> Result<Self>;
    fn try_div(self, rhs: Self) -> Result<Self>;
    fn try_mul(self, rhs: Self) -> Result<Self>;
    fn try_rem(self, rhs: Self) -> Result<Self>;
}

/// Macro to create try-able arithmetic operations for a numeric type
/// Note - not using num_traits because U256 doesn't implement it
macro_rules! create_try_math {
    ($type: ty) => {
        impl TryMath for $type {
            fn try_add(self, rhs: Self) -> Result<Self> {
                self.checked_add(rhs).ok_or_else(|| {
                    msg!("Calculation failure: {}.try_add({})", self, rhs);
                    error!(SwapError::CalculationFailure)
                })
            }

            fn try_sub(self, rhs: Self) -> Result<Self> {
                self.checked_sub(rhs).ok_or_else(|| {
                    msg!("Calculation failure: {}.try_sub({})", self, rhs);
                    error!(SwapError::CalculationFailure)
                })
            }

            fn try_div(self, rhs: Self) -> Result<Self> {
                self.checked_div(rhs).ok_or_else(|| {
                    msg!("Calculation failure: {}.try_div({})", self, rhs);
                    error!(SwapError::CalculationFailure)
                })
            }

            fn try_mul(self, rhs: Self) -> Result<Self> {
                self.checked_mul(rhs).ok_or_else(|| {
                    msg!("Calculation failure: {}.try_mul({})", self, rhs);
                    error!(SwapError::CalculationFailure)
                })
            }

            fn try_rem(self, rhs: Self) -> Result<Self> {
                self.checked_rem(rhs).ok_or_else(|| {
                    msg!("Calculation failure: {}.try_rem({})", self, rhs);
                    error!(SwapError::CalculationFailure)
                })
            }
        }
    };
}

create_try_math!(u8);
create_try_math!(u64);
create_try_math!(u128);
create_try_math!(U256);

pub trait AbsDiff {
    fn abs_diff(self, rhs: Self) -> Self;
}

impl AbsDiff for U256 {
    fn abs_diff(self, rhs: Self) -> Self {
        if self > rhs {
            self - rhs
        } else {
            rhs - self
        }
    }
}
