#![allow(clippy::integer_arithmetic)]
#![allow(clippy::result_large_err)] // todo - reduce Error size
// #![deny(missing_docs)]

//! An AMM program for the Solana blockchain.

pub mod constraints;
pub mod curve;
pub mod error;
pub mod instructions;
pub mod ix;
pub mod processor;
pub mod state;
pub mod utils;

// Export current sdk types for downstream users building with a different sdk version
pub use anchor_lang;

use anchor_lang::prelude::*;

use curve::fees::Fees;
pub use instructions::*;

declare_id!("SwapsVeCiPHMUAtzQWZw7RjsKjgCjhwU55QGu4U1Szw");

#[program]
pub mod hyperplane {
    use super::*;
    use crate::processor::Processor;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        curve_parameters: CurveParameters,
        fees: Fees,
        initial_supply_a: u64,
        initial_supply_b: u64,
    ) -> Result<()> {
        instructions::initialize_pool::handler(
            ctx,
            curve_parameters,
            fees,
            initialize_pool::InitialSupply {
                initial_supply_a,
                initial_supply_b,
            },
        )
    }

    pub fn swap(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<()> {
        instructions::swap::handler(ctx, amount_in, minimum_amount_out)
    }

    pub fn fallback(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> Result<()> {
        Processor::process(program_id, accounts, input).map_err(|e| e.into())
    }
}
