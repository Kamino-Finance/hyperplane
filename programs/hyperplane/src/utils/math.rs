use crate::error::SwapError;
use anchor_lang::prelude::msg;

pub fn to_u128(val: u64) -> u128 {
    val.into()
}

pub fn to_u64(val: u128) -> Result<u64, SwapError> {
    val.try_into().map_err(|_| {
        msg!("Unable to convert u128 to u64: {}", val);
        SwapError::ConversionFailure
    })
}
