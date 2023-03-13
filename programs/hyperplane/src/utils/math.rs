use crate::error::SwapError;
use anchor_lang::prelude::msg;
use anchor_lang::{error, Result};
use spl_math::precise_number::PreciseNumber;
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

pub trait TryMathRef
where
    Self: Sized,
{
    fn try_add(&self, rhs: &Self) -> Result<Self>;
    fn try_sub(&self, rhs: &Self) -> Result<Self>;
    fn try_div(&self, rhs: &Self) -> Result<Self>;
    fn try_mul(&self, rhs: &Self) -> Result<Self>;
    fn try_floor(&self) -> Result<Self>;
    fn try_ceil(&self) -> Result<Self>;
    fn try_sqrt(&self) -> Result<Self>;
    fn try_pow(&self, exponent: u128) -> Result<Self>;
    fn try_to_imprecise(&self) -> Result<u128>;
}

impl TryMathRef for PreciseNumber {
    fn try_add(&self, rhs: &Self) -> Result<Self> {
        self.checked_add(rhs).ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_add({:?})", self, rhs);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_sub(&self, rhs: &Self) -> Result<Self> {
        self.checked_sub(rhs).ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_sub({:?})", self, rhs);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_div(&self, rhs: &Self) -> Result<Self> {
        self.checked_div(rhs).ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_div({:?})", self, rhs);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_mul(&self, rhs: &Self) -> Result<Self> {
        self.checked_mul(rhs).ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_mul({:?})", self, rhs);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_floor(&self) -> Result<Self> {
        self.floor().ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_floor()", self);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_ceil(&self) -> Result<Self> {
        self.ceiling().ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_ceil()", self);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_sqrt(&self) -> Result<Self> {
        self.sqrt().ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_sqrt()", self);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_pow(&self, exponent: u128) -> Result<Self> {
        self.checked_pow(exponent).ok_or_else(|| {
            msg!("Calculation failure: {:?}.try_pow({})", self, exponent);
            error!(SwapError::CalculationFailure)
        })
    }

    fn try_to_imprecise(&self) -> Result<u128> {
        self.to_imprecise().ok_or_else(|| {
            msg!("Calculation failure: {:?}.to_imprecise()", self);
            error!(SwapError::CalculationFailure)
        })
    }
}

pub trait TryCeilDiv
where
    Self: Sized,
{
    fn try_ceil_div(self, denominator: Self) -> Result<(Self, Self)>;
}

impl TryCeilDiv for U256 {
    fn try_ceil_div(self, denominator: Self) -> Result<(Self, Self)> {
        use spl_math::checked_ceil_div::CheckedCeilDiv;
        self.checked_ceil_div(denominator).ok_or_else(|| {
            msg!(
                "Calculation failure: {}.try_ceil_div({})",
                self,
                denominator
            );
            error!(SwapError::CalculationFailure)
        })
    }
}

impl TryCeilDiv for u128 {
    fn try_ceil_div(self, denominator: Self) -> Result<(Self, Self)> {
        use spl_math::checked_ceil_div::CheckedCeilDiv;
        self.checked_ceil_div(denominator).ok_or_else(|| {
            msg!(
                "Calculation failure: {}.try_ceil_div({})",
                self,
                denominator
            );
            error!(SwapError::CalculationFailure)
        })
    }
}

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

pub fn decimals_to_factor(source_decimals: u8, destination_decimals: u8) -> Result<u64> {
    Ok(10_u64.pow((destination_decimals.saturating_sub(source_decimals)) as u32))
}

pub trait TryNew
where
    Self: Sized,
{
    fn try_new(new: u128) -> Result<Self>;
}

impl TryNew for PreciseNumber {
    fn try_new(new: u128) -> Result<Self> {
        PreciseNumber::new(new).ok_or_else(|| {
            msg!("Instantiation failure: PreciseNumber::new({})", new);
            error!(SwapError::CalculationFailure)
        })
    }
}
