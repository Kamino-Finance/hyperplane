//! State transition types

use crate::{
    curve::{base::CurveType, fees::Fees},
    error::SwapError,
};
use anchor_lang::prelude::{borsh, msg, ProgramError, Pubkey};
use anchor_lang::solana_program::account_info::AccountInfo;
use anchor_lang::{account, zero_copy, AnchorDeserialize, AnchorSerialize};
use enum_dispatch::enum_dispatch;
use spl_token_2022::{
    extension::StateWithExtensions,
    state::{Account, AccountState},
};

const DISCRIMINATOR_SIZE: usize = 8;

/// Trait representing access to program state across all versions
#[enum_dispatch]
pub trait SwapState {
    /// Is the swap initialized, with data written to it
    fn is_initialized(&self) -> bool;
    /// Bump seed used to generate the program address / authority
    fn bump_seed(&self) -> u8;
    /// Token program ID associated with the swap
    fn token_program_id(&self) -> &Pubkey;
    /// Address of token A liquidity account
    fn token_a_account(&self) -> &Pubkey;
    /// Address of token B liquidity account
    fn token_b_account(&self) -> &Pubkey;
    /// Address of pool token mint
    fn pool_mint(&self) -> &Pubkey;

    /// Address of token A mint
    fn token_a_mint(&self) -> &Pubkey;
    /// Address of token B mint
    fn token_b_mint(&self) -> &Pubkey;

    /// Address of pool fee account
    fn pool_fee_account(&self) -> &Pubkey;
    /// Check if the pool fee info is a valid token program account
    /// capable of receiving tokens from the mint.
    fn check_pool_fee_info(&self, pool_fee_info: &AccountInfo) -> Result<(), ProgramError>;

    /// Fees associated with swap
    fn fees(&self) -> &Fees;
    fn curve_type(&self) -> CurveType;
}

/// Program states

#[account(zero_copy)]
#[derive(Debug, Default, PartialEq)]
pub struct SwapPool {
    /// Initialized state.
    pub is_initialized: u64,

    /// Pool authority
    pub pool_authority: Pubkey,
    /// Bump seed used in pool authority program address
    pub pool_authority_bump_seed: u64,

    /// Program ID of the tokens being exchanged.
    pub token_program_id: Pubkey, // todo - elliot - probably remove and just check mint

    /// Token A
    pub token_a_vault: Pubkey,
    /// Token B
    pub token_b_vault: Pubkey,

    /// Pool tokens are issued when A or B tokens are deposited
    /// Pool tokens can be withdrawn back to the original A or B token
    pub pool_token_mint: Pubkey,

    /// Mint information for token A
    pub token_a_mint: Pubkey,
    /// Mint information for token B
    pub token_b_mint: Pubkey,

    /// Pool token account to receive trading and / or withdrawal fees
    pub pool_token_fees_vault: Pubkey,

    /// All fee information
    pub fees: Fees,

    /// Swap curve account type, to assist in deserializing the swap account and used by the SwapCurve, which
    /// calculates swaps, deposits, and withdrawals
    pub curve_type: u64,
    /// The swap curve account address for this pool
    pub swap_curve: Pubkey,
}

impl SwapPool {
    pub const LEN: usize = DISCRIMINATOR_SIZE + 376; // 8 + 376 = 384
}

impl SwapState for SwapPool {
    fn is_initialized(&self) -> bool {
        self.is_initialized == 1
    }

    fn bump_seed(&self) -> u8 {
        u8::try_from(self.pool_authority_bump_seed).unwrap()
    }

    fn token_program_id(&self) -> &Pubkey {
        &self.token_program_id
    }

    fn token_a_account(&self) -> &Pubkey {
        &self.token_a_vault
    }

    fn token_b_account(&self) -> &Pubkey {
        &self.token_b_vault
    }

    fn pool_mint(&self) -> &Pubkey {
        &self.pool_token_mint
    }

    fn token_a_mint(&self) -> &Pubkey {
        &self.token_a_mint
    }

    fn token_b_mint(&self) -> &Pubkey {
        &self.token_b_mint
    }

    fn pool_fee_account(&self) -> &Pubkey {
        &self.pool_token_fees_vault
    }

    fn check_pool_fee_info(&self, pool_fee_info: &AccountInfo) -> Result<(), ProgramError> {
        let data = &pool_fee_info.data.borrow();
        let token_account =
            StateWithExtensions::<Account>::unpack(data).map_err(|err| match err {
                ProgramError::InvalidAccountData | ProgramError::UninitializedAccount => {
                    SwapError::InvalidFeeAccount.into()
                }
                _ => err,
            })?;
        if pool_fee_info.owner != &self.token_program_id
            || token_account.base.state != AccountState::Initialized
            || token_account.base.mint != self.pool_token_mint
        {
            msg!("Pool fee account is not owned by token program, is not initialized, or does not match stake pool's mint");
            return Err(SwapError::InvalidFeeAccount.into());
        }
        Ok(())
    }

    fn fees(&self) -> &Fees {
        &self.fees
    }

    fn curve_type(&self) -> CurveType {
        CurveType::try_from(self.curve_type).unwrap()
    }
}

pub struct Curve {}
impl Curve {
    pub const LEN: usize = DISCRIMINATOR_SIZE + (16 * 8);
}

#[account]
#[derive(Debug, PartialEq, Default)]
pub struct ConstantPriceCurve {
    /// Amount of token A required to get 1 token B
    pub token_b_price: u64,
    pub _padding: [u64; 15],
}

#[account]
#[derive(Debug, PartialEq, Default)]
pub struct ConstantProductCurve {
    pub _padding: [u64; 16],
}

#[account]
#[derive(Debug, PartialEq, Default)]
pub struct OffsetCurve {
    /// Amount to offset the token B liquidity account
    pub token_b_offset: u64,
    pub _padding: [u64; 15],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_pool_state_size() {
        let x = std::mem::size_of::<SwapPool>();
        assert_eq!(x, SwapPool::LEN - DISCRIMINATOR_SIZE);
    }
}
