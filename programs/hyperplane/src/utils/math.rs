use crate::error::SwapError;
use anchor_lang::error;
use anchor_lang::prelude::*;

pub fn to_u128(val: u64) -> u128 {
    val.into()
}

pub fn to_u64(val: u128) -> Result<u64> {
    val.try_into().map_err(|_| {
        msg!("Unable to convert u128 to u64: {}", val);
        error!(SwapError::ConversionFailure)
    })
}
