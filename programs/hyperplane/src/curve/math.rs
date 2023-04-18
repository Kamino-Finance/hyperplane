use anchor_lang::prelude::*;

use crate::{
    curve::calculator::{RoundDirection, TradingTokenResult},
    try_math,
    utils::math::TryMath,
};

/// Get the amount of trading tokens for the given amount of pool tokens,
/// provided the total trading tokens and supply of pool tokens.
///
/// This implementation is a simple ratio calculation for how many
/// trading tokens correspond to a certain number of pool tokens
pub fn pool_tokens_to_trading_tokens(
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
            // Also check for 0 token A and B amount to avoid taking too much
            // for tiny amounts of pool tokens.  For example, if someone asks
            // for 1 pool token, which is worth 0.01 token A, we avoid the
            // ceiling of taking 1 token A and instead return 0, for it to be
            // rejected later in processing.
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
