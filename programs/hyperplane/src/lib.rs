#![allow(clippy::integer_arithmetic)]
#![deny(missing_docs)]

//! An AMM program for the Solana blockchain.

pub mod constraints;
pub mod curve;
pub mod error;
pub mod ix;
pub mod processor;
pub mod state;

// Export current sdk types for downstream users building with a different sdk version
pub use anchor_lang;

use anchor_lang::prelude::*;

declare_id!("SwapsVeCiPHMUAtzQWZw7RjsKjgCjhwU55QGu4U1Szw");

#[program]
mod hyperplane {
    use super::*;
    use crate::processor::Processor;

    pub fn fallback(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> Result<()> {
        Processor::process(program_id, accounts, input).unwrap();
        Ok(())
    }
}
