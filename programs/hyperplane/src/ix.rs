//! Instruction types

#![allow(clippy::too_many_arguments)]

use anchor_lang::Id;
use anchor_lang::ToAccountMetas;
use std::convert::TryInto;
use std::mem::size_of;

use anchor_lang::prelude::{borsh, Rent, System};
use anchor_lang::solana_program::sysvar::SysvarId;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
};
#[cfg(feature = "fuzz")]
use arbitrary::Arbitrary;

use crate::curve::fees::Fees;
use crate::error::SwapError;
use crate::instructions::CurveParameters;
use crate::InitialSupply;
use sha2::{Digest, Sha256};

/// Initialize instruction data
#[derive(Debug, PartialEq)]
pub struct Initialize {
    /// all swap fees
    pub fees: Fees,
    /// swap curve info for pool, including CurveType and anything
    /// else that may be required
    pub curve: CurveParameters,
}

/// Swap instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq)]
pub struct Swap {
    /// SOURCE amount to transfer, output to DESTINATION is based on the exchange rate
    pub amount_in: u64,
    /// Minimum amount of DESTINATION token to output, prevents excessive slippage
    pub minimum_amount_out: u64,
}

/// DepositAllTokenTypes instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq)]
pub struct DepositAllTokenTypes {
    /// Pool token amount to transfer. token_a and token_b amount are set by
    /// the current exchange rate and size of the pool
    pub pool_token_amount: u64,
    /// Maximum token A amount to deposit, prevents excessive slippage
    pub maximum_token_a_amount: u64,
    /// Maximum token B amount to deposit, prevents excessive slippage
    pub maximum_token_b_amount: u64,
}

/// WithdrawAllTokenTypes instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq)]
pub struct WithdrawAllTokenTypes {
    /// Amount of pool tokens to burn. User receives an output of token a
    /// and b based on the percentage of the pool tokens that are returned.
    pub pool_token_amount: u64,
    /// Minimum amount of token A to receive, prevents excessive slippage
    pub minimum_token_a_amount: u64,
    /// Minimum amount of token B to receive, prevents excessive slippage
    pub minimum_token_b_amount: u64,
}

/// Deposit one token type, exact amount in instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq)]
pub struct DepositSingleTokenTypeExactAmountIn {
    /// Token amount to deposit
    pub source_token_amount: u64,
    /// Pool token amount to receive in exchange. The amount is set by
    /// the current exchange rate and size of the pool
    pub minimum_pool_token_amount: u64,
}

/// WithdrawSingleTokenTypeExactAmountOut instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq)]
pub struct WithdrawSingleTokenTypeExactAmountOut {
    /// Amount of token A or B to receive
    pub destination_token_amount: u64,
    /// Maximum amount of pool tokens to burn. User receives an output of token A
    /// or B based on the percentage of the pool tokens that are returned.
    pub maximum_pool_token_amount: u64,
}

/// Instructions supported by the token swap program.
#[repr(C)]
#[derive(Debug, PartialEq)]
pub enum SwapInstruction {
    ///   Initializes a new swap pool
    ///
    ///   0. `[writable, signer]` New Token-swap to create.
    ///   1. `[]` swap authority derived from `create_program_address(&[Token-swap account])`
    ///   2. `[]` token_a Account. Must be non zero, owned by swap authority.
    ///   3. `[]` token_b Account. Must be non zero, owned by swap authority.
    ///   4. `[writable]` Pool Token Mint. Must be empty, owned by swap authority.
    ///   5. `[]` Pool Token Account to deposit trading and withdraw fees.
    ///   Must be empty, not owned by swap authority
    ///   6. `[writable]` Pool Token Account to deposit the initial pool token
    ///   supply.  Must be empty, not owned by swap authority.
    ///   7. `[]` Pool Token program id
    InitializePool {
        initial_supply_a: u64,
        initial_supply_b: u64,
        fees: Fees,
        curve: CurveParameters,
    },

    ///   Swap the tokens in the pool.
    ///
    ///   0. `[]` Token-swap
    ///   1. `[]` swap authority
    ///   2. `[]` user transfer authority
    ///   3. `[writable]` token_(A|B) SOURCE Account, amount is transferable by user transfer authority,
    ///   4. `[writable]` token_(A|B) Base Account to swap INTO.  Must be the SOURCE token.
    ///   5. `[writable]` token_(A|B) Base Account to swap FROM.  Must be the DESTINATION token.
    ///   6. `[writable]` token_(A|B) DESTINATION Account assigned to USER as the owner.
    ///   7. `[writable]` Pool token mint, to generate trading fees
    ///   8. `[writable]` Fee account, to receive trading fees
    ///   9. `[]` Token (A|B) SOURCE mint
    ///   10. `[]` Token (A|B) DESTINATION mint
    ///   11. `[]` Token (A|B) SOURCE program id
    ///   12. `[]` Token (A|B) DESTINATION program id
    ///   13. `[]` Pool Token program id
    ///   14. `[optional, writable]` Host fee account to receive additional trading fees
    Swap {
        amount_in: u64,
        minimum_amount_out: u64,
    },

    ///   Deposit both types of tokens into the pool.  The output is a "pool"
    ///   token representing ownership in the pool. Inputs are converted to
    ///   the current ratio.
    ///
    ///   0. `[]` Token-swap
    ///   1. `[]` swap authority
    ///   2. `[]` user transfer authority
    ///   3. `[writable]` token_a user transfer authority can transfer amount,
    ///   4. `[writable]` token_b user transfer authority can transfer amount,
    ///   5. `[writable]` token_a Base Account to deposit into.
    ///   6. `[writable]` token_b Base Account to deposit into.
    ///   7. `[writable]` Pool MINT account, swap authority is the owner.
    ///   8. `[writable]` Pool Account to deposit the generated tokens, user is the owner.
    ///   9. `[]` Token A mint
    ///   10. `[]` Token B mint
    ///   11. `[]` Token A program id
    ///   12. `[]` Token B program id
    ///   13. `[]` Pool Token program id
    DepositAllTokenTypes {
        pool_token_amount: u64,
        maximum_token_a_amount: u64,
        maximum_token_b_amount: u64,
    },

    ///   Withdraw both types of tokens from the pool at the current ratio, given
    ///   pool tokens.  The pool tokens are burned in exchange for an equivalent
    ///   amount of token A and B.
    ///
    ///   0. `[]` Token-swap
    ///   1. `[]` swap authority
    ///   2. `[]` user transfer authority
    ///   3. `[writable]` Pool mint account, swap authority is the owner
    ///   4. `[writable]` SOURCE Pool account, amount is transferable by user transfer authority.
    ///   5. `[writable]` token_a Swap Account to withdraw FROM.
    ///   6. `[writable]` token_b Swap Account to withdraw FROM.
    ///   7. `[writable]` token_a user Account to credit.
    ///   8. `[writable]` token_b user Account to credit.
    ///   9. `[writable]` Fee account, to receive withdrawal fees
    ///   10. `[]` Token A mint
    ///   11. `[]` Token B mint
    ///   12. `[]` Pool Token program id
    ///   13. `[]` Token A program id
    ///   14. `[]` Token B program id
    WithdrawAllTokenTypes {
        pool_token_amount: u64,
        minimum_token_a_amount: u64,
        minimum_token_b_amount: u64,
    },

    ///   Deposit one type of tokens into the pool.  The output is a "pool" token
    ///   representing ownership into the pool. Input token is converted as if
    ///   a swap and deposit all token types were performed.
    ///
    ///   0. `[]` Token-swap
    ///   1. `[]` swap authority
    ///   2. `[]` user transfer authority
    ///   3. `[writable]` token_(A|B) SOURCE Account, amount is transferable by user transfer authority,
    ///   4. `[writable]` token_a Swap Account, may deposit INTO.
    ///   5. `[writable]` token_b Swap Account, may deposit INTO.
    ///   6. `[writable]` Pool MINT account, swap authority is the owner.
    ///   7. `[writable]` Pool Account to deposit the generated tokens, user is the owner.
    ///   8. `[]` Token (A|B) SOURCE mint
    ///   9. `[]` Token (A|B) SOURCE program id
    ///   10. `[]` Pool Token program id
    DepositSingleTokenTypeExactAmountIn {
        source_token_amount: u64,
        minimum_pool_token_amount: u64,
    },

    ///   Withdraw one token type from the pool at the current ratio given the
    ///   exact amount out expected.
    ///
    ///   0. `[]` Token-swap
    ///   1. `[]` swap authority
    ///   2. `[]` user transfer authority
    ///   3. `[writable]` Pool mint account, swap authority is the owner
    ///   4. `[writable]` SOURCE Pool account, amount is transferable by user transfer authority.
    ///   5. `[writable]` token_a Swap Account to potentially withdraw from.
    ///   6. `[writable]` token_b Swap Account to potentially withdraw from.
    ///   7. `[writable]` token_(A|B) User Account to credit
    ///   8. `[writable]` Fee account, to receive withdrawal fees
    ///   9. `[]` Token (A|B) DESTINATION mint
    ///   10. `[]` Pool Token program id
    ///   11. `[]` Token (A|B) DESTINATION program id
    WithdrawSingleTokenTypeExactAmountOut {
        destination_token_amount: u64,
        maximum_pool_token_amount: u64,
    },
}

impl SwapInstruction {
    /// Unpacks a byte buffer into a [SwapInstruction](enum.SwapInstruction.html).
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = input.split_first().ok_or(SwapError::InvalidInstruction)?;
        Ok(match tag {
            0 => {
                // if rest.len() >= Fees::LEN {
                //     let (fees, rest) = rest.split_at(Fees::LEN);
                //     let fees = Fees::unpack_unchecked(fees)?;
                //     let swap_curve = T::try_deserialize(rest)?;
                //     Self::Initialize(Initialize { fees, swap_curve })
                // } else {
                //     return Err(SwapError::InvalidInstruction.into());
                // }
                panic!("Initialize endpoint has been migrated to anchor");
            }
            1 => {
                let (amount_in, rest) = Self::unpack_u64(rest)?;
                let (minimum_amount_out, _rest) = Self::unpack_u64(rest)?;
                Self::Swap {
                    amount_in,
                    minimum_amount_out,
                }
            }
            2 => {
                let (pool_token_amount, rest) = Self::unpack_u64(rest)?;
                let (maximum_token_a_amount, rest) = Self::unpack_u64(rest)?;
                let (maximum_token_b_amount, _rest) = Self::unpack_u64(rest)?;
                Self::DepositAllTokenTypes {
                    pool_token_amount,
                    maximum_token_a_amount,
                    maximum_token_b_amount,
                }
            }
            3 => {
                let (pool_token_amount, rest) = Self::unpack_u64(rest)?;
                let (minimum_token_a_amount, rest) = Self::unpack_u64(rest)?;
                let (minimum_token_b_amount, _rest) = Self::unpack_u64(rest)?;
                Self::WithdrawAllTokenTypes {
                    pool_token_amount,
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                }
            }
            4 => {
                let (source_token_amount, rest) = Self::unpack_u64(rest)?;
                let (minimum_pool_token_amount, _rest) = Self::unpack_u64(rest)?;
                Self::DepositSingleTokenTypeExactAmountIn {
                    source_token_amount,
                    minimum_pool_token_amount,
                }
            }
            5 => {
                let (destination_token_amount, rest) = Self::unpack_u64(rest)?;
                let (maximum_pool_token_amount, _rest) = Self::unpack_u64(rest)?;
                Self::WithdrawSingleTokenTypeExactAmountOut {
                    destination_token_amount,
                    maximum_pool_token_amount,
                }
            }
            _ => return Err(SwapError::InvalidInstruction.into()),
        })
    }

    fn unpack_u64(input: &[u8]) -> Result<(u64, &[u8]), ProgramError> {
        if input.len() >= 8 {
            let (amount, rest) = input.split_at(8);
            let amount = amount
                .get(..8)
                .and_then(|slice| slice.try_into().ok())
                .map(u64::from_le_bytes)
                .ok_or(SwapError::InvalidInstruction)?;
            Ok((amount, rest))
        } else {
            Err(SwapError::InvalidInstruction.into())
        }
    }

    /// Packs a [SwapInstruction](enum.SwapInstruction.html) into a byte buffer.
    pub fn pack(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(size_of::<Self>());
        match self {
            Self::InitializePool {
                curve,
                fees,
                initial_supply_a,
                initial_supply_b,
            } => {
                buf.extend_from_slice(&dispatch_sig("global", "initialize_pool"));
                let curve = borsh::to_vec(&curve).unwrap();
                buf.extend_from_slice(&curve);
                let fees = borsh::to_vec(&fees).unwrap();
                buf.extend_from_slice(&fees);
                buf.extend_from_slice(&initial_supply_a.to_le_bytes());
                buf.extend_from_slice(&initial_supply_b.to_le_bytes());
            }
            Self::Swap {
                amount_in,
                minimum_amount_out,
            } => {
                buf.push(1);
                buf.extend_from_slice(&amount_in.to_le_bytes());
                buf.extend_from_slice(&minimum_amount_out.to_le_bytes());
            }
            Self::DepositAllTokenTypes {
                pool_token_amount,
                maximum_token_a_amount,
                maximum_token_b_amount,
            } => {
                buf.push(2);
                buf.extend_from_slice(&pool_token_amount.to_le_bytes());
                buf.extend_from_slice(&maximum_token_a_amount.to_le_bytes());
                buf.extend_from_slice(&maximum_token_b_amount.to_le_bytes());
            }
            Self::WithdrawAllTokenTypes {
                pool_token_amount,
                minimum_token_a_amount,
                minimum_token_b_amount,
            } => {
                buf.push(3);
                buf.extend_from_slice(&pool_token_amount.to_le_bytes());
                buf.extend_from_slice(&minimum_token_a_amount.to_le_bytes());
                buf.extend_from_slice(&minimum_token_b_amount.to_le_bytes());
            }
            Self::DepositSingleTokenTypeExactAmountIn {
                source_token_amount,
                minimum_pool_token_amount,
            } => {
                buf.push(4);
                buf.extend_from_slice(&source_token_amount.to_le_bytes());
                buf.extend_from_slice(&minimum_pool_token_amount.to_le_bytes());
            }
            Self::WithdrawSingleTokenTypeExactAmountOut {
                destination_token_amount,
                maximum_pool_token_amount,
            } => {
                buf.push(5);
                buf.extend_from_slice(&destination_token_amount.to_le_bytes());
                buf.extend_from_slice(&maximum_pool_token_amount.to_le_bytes());
            }
        }
        buf
    }
}

/// Creates an 'initialize' instruction.
pub fn initialize_pool(
    program_id: &Pubkey,
    admin_authority: &Pubkey,
    pool: &Pubkey,
    swap_curve: &Pubkey,
    token_a_mint: &Pubkey,
    token_b_mint: &Pubkey,
    token_a_vault: &Pubkey,
    token_b_vault: &Pubkey,
    pool_authority: &Pubkey,
    pool_token_mint: &Pubkey,
    pool_token_fees_vault: &Pubkey,
    admin_authority_token_a_ata: &Pubkey,
    admin_authority_token_b_ata: &Pubkey,
    admin_authority_pool_token_ata: &Pubkey,
    pool_token_program_id: &Pubkey,
    token_a_program_id: &Pubkey,
    token_b_program_id: &Pubkey,
    fees: Fees,
    initial_supply: InitialSupply,
    curve: CurveParameters,
) -> Result<Instruction, ProgramError> {
    let data = SwapInstruction::InitializePool {
        initial_supply_a: initial_supply.initial_supply_a,
        initial_supply_b: initial_supply.initial_supply_b,
        fees,
        curve,
    }
    .pack();

    let accounts = super::accounts::InitializePool {
        admin_authority: *admin_authority,
        pool: *pool,
        swap_curve: *swap_curve,
        pool_authority: *pool_authority,
        token_a_mint: *token_a_mint,
        token_b_mint: *token_b_mint,
        token_a_vault: *token_a_vault,
        token_b_vault: *token_b_vault,
        pool_token_mint: *pool_token_mint,
        pool_token_fees_vault: *pool_token_fees_vault,
        admin_authority_token_a_ata: *admin_authority_token_a_ata,
        admin_authority_token_b_ata: *admin_authority_token_b_ata,
        admin_authority_pool_token_ata: *admin_authority_pool_token_ata,
        system_program: System::id(),
        rent: Rent::id(),
        pool_token_program: *pool_token_program_id,
        token_a_token_program: *token_a_program_id,
        token_b_token_program: *token_b_program_id,
    }
    .to_account_metas(None);

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'deposit_all_token_types' instruction.
pub fn deposit_all_token_types(
    program_id: &Pubkey,
    token_a_program_id: &Pubkey,
    token_b_program_id: &Pubkey,
    pool_token_program_id: &Pubkey,
    swap_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    user_transfer_authority_pubkey: &Pubkey,
    deposit_token_a_pubkey: &Pubkey,
    deposit_token_b_pubkey: &Pubkey,
    swap_token_a_pubkey: &Pubkey,
    swap_token_b_pubkey: &Pubkey,
    pool_mint_pubkey: &Pubkey,
    destination_pubkey: &Pubkey,
    token_a_mint_pubkey: &Pubkey,
    token_b_mint_pubkey: &Pubkey,
    swap_curve: &Pubkey,
    instruction: DepositAllTokenTypes,
) -> Result<Instruction, ProgramError> {
    let data = SwapInstruction::DepositAllTokenTypes {
        pool_token_amount: instruction.pool_token_amount,
        maximum_token_a_amount: instruction.maximum_token_a_amount,
        maximum_token_b_amount: instruction.maximum_token_b_amount,
    }
    .pack();

    let accounts = vec![
        AccountMeta::new_readonly(*swap_pubkey, false),
        AccountMeta::new_readonly(*authority_pubkey, false),
        AccountMeta::new_readonly(*user_transfer_authority_pubkey, true),
        AccountMeta::new(*deposit_token_a_pubkey, false),
        AccountMeta::new(*deposit_token_b_pubkey, false),
        AccountMeta::new(*swap_token_a_pubkey, false),
        AccountMeta::new(*swap_token_b_pubkey, false),
        AccountMeta::new(*pool_mint_pubkey, false),
        AccountMeta::new(*destination_pubkey, false),
        AccountMeta::new_readonly(*token_a_mint_pubkey, false),
        AccountMeta::new_readonly(*token_b_mint_pubkey, false),
        AccountMeta::new_readonly(*token_a_program_id, false),
        AccountMeta::new_readonly(*token_b_program_id, false),
        AccountMeta::new_readonly(*pool_token_program_id, false),
        AccountMeta::new_readonly(*swap_curve, false),
    ];

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'withdraw_all_token_types' instruction.
pub fn withdraw_all_token_types(
    program_id: &Pubkey,
    pool_token_program_id: &Pubkey,
    token_a_program_id: &Pubkey,
    token_b_program_id: &Pubkey,
    swap_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    user_transfer_authority_pubkey: &Pubkey,
    pool_mint_pubkey: &Pubkey,
    fee_account_pubkey: &Pubkey,
    source_pubkey: &Pubkey,
    swap_token_a_pubkey: &Pubkey,
    swap_token_b_pubkey: &Pubkey,
    destination_token_a_pubkey: &Pubkey,
    destination_token_b_pubkey: &Pubkey,
    token_a_mint_pubkey: &Pubkey,
    token_b_mint_pubkey: &Pubkey,
    swap_curve: &Pubkey,
    instruction: WithdrawAllTokenTypes,
) -> Result<Instruction, ProgramError> {
    let data = SwapInstruction::WithdrawAllTokenTypes {
        pool_token_amount: instruction.pool_token_amount,
        minimum_token_a_amount: instruction.minimum_token_a_amount,
        minimum_token_b_amount: instruction.minimum_token_b_amount,
    }
    .pack();

    let accounts = vec![
        AccountMeta::new_readonly(*swap_pubkey, false),
        AccountMeta::new_readonly(*authority_pubkey, false),
        AccountMeta::new_readonly(*user_transfer_authority_pubkey, true),
        AccountMeta::new(*pool_mint_pubkey, false),
        AccountMeta::new(*source_pubkey, false),
        AccountMeta::new(*swap_token_a_pubkey, false),
        AccountMeta::new(*swap_token_b_pubkey, false),
        AccountMeta::new(*destination_token_a_pubkey, false),
        AccountMeta::new(*destination_token_b_pubkey, false),
        AccountMeta::new(*fee_account_pubkey, false),
        AccountMeta::new_readonly(*token_a_mint_pubkey, false),
        AccountMeta::new_readonly(*token_b_mint_pubkey, false),
        AccountMeta::new_readonly(*pool_token_program_id, false),
        AccountMeta::new_readonly(*token_a_program_id, false),
        AccountMeta::new_readonly(*token_b_program_id, false),
        AccountMeta::new_readonly(*swap_curve, false),
    ];

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'deposit_single_token_type_exact_amount_in' instruction.
pub fn deposit_single_token_type_exact_amount_in(
    program_id: &Pubkey,
    source_token_program_id: &Pubkey,
    pool_token_program_id: &Pubkey,
    swap_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    user_transfer_authority_pubkey: &Pubkey,
    source_token_pubkey: &Pubkey,
    swap_token_a_pubkey: &Pubkey,
    swap_token_b_pubkey: &Pubkey,
    pool_mint_pubkey: &Pubkey,
    destination_pubkey: &Pubkey,
    source_mint_pubkey: &Pubkey,
    swap_curve: &Pubkey,
    instruction: DepositSingleTokenTypeExactAmountIn,
) -> Result<Instruction, ProgramError> {
    let data = SwapInstruction::DepositSingleTokenTypeExactAmountIn {
        source_token_amount: instruction.source_token_amount,
        minimum_pool_token_amount: instruction.minimum_pool_token_amount,
    }
    .pack();

    let accounts = vec![
        AccountMeta::new_readonly(*swap_pubkey, false),
        AccountMeta::new_readonly(*authority_pubkey, false),
        AccountMeta::new_readonly(*user_transfer_authority_pubkey, true),
        AccountMeta::new(*source_token_pubkey, false),
        AccountMeta::new(*swap_token_a_pubkey, false),
        AccountMeta::new(*swap_token_b_pubkey, false),
        AccountMeta::new(*pool_mint_pubkey, false),
        AccountMeta::new(*destination_pubkey, false),
        AccountMeta::new_readonly(*source_mint_pubkey, false),
        AccountMeta::new_readonly(*source_token_program_id, false),
        AccountMeta::new_readonly(*pool_token_program_id, false),
        AccountMeta::new_readonly(*swap_curve, false),
    ];

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'withdraw_single_token_type_exact_amount_out' instruction.
pub fn withdraw_single_token_type_exact_amount_out(
    program_id: &Pubkey,
    pool_token_program_id: &Pubkey,
    destination_token_program_id: &Pubkey,
    swap_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    user_transfer_authority_pubkey: &Pubkey,
    pool_mint_pubkey: &Pubkey,
    fee_account_pubkey: &Pubkey,
    pool_token_source_pubkey: &Pubkey,
    swap_token_a_pubkey: &Pubkey,
    swap_token_b_pubkey: &Pubkey,
    destination_pubkey: &Pubkey,
    destination_mint_pubkey: &Pubkey,
    swap_curve: &Pubkey,
    instruction: WithdrawSingleTokenTypeExactAmountOut,
) -> Result<Instruction, ProgramError> {
    let data = SwapInstruction::WithdrawSingleTokenTypeExactAmountOut {
        destination_token_amount: instruction.destination_token_amount,
        maximum_pool_token_amount: instruction.maximum_pool_token_amount,
    }
    .pack();

    let accounts = vec![
        AccountMeta::new_readonly(*swap_pubkey, false),
        AccountMeta::new_readonly(*authority_pubkey, false),
        AccountMeta::new_readonly(*user_transfer_authority_pubkey, true),
        AccountMeta::new(*pool_mint_pubkey, false),
        AccountMeta::new(*pool_token_source_pubkey, false),
        AccountMeta::new(*swap_token_a_pubkey, false),
        AccountMeta::new(*swap_token_b_pubkey, false),
        AccountMeta::new(*destination_pubkey, false),
        AccountMeta::new(*fee_account_pubkey, false),
        AccountMeta::new_readonly(*destination_mint_pubkey, false),
        AccountMeta::new_readonly(*pool_token_program_id, false),
        AccountMeta::new_readonly(*destination_token_program_id, false),
        AccountMeta::new_readonly(*swap_curve, false),
    ];

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'swap' instruction.
pub fn swap(
    program_id: &Pubkey,
    source_token_program_id: &Pubkey,
    destination_token_program_id: &Pubkey,
    pool_token_program_id: &Pubkey,
    swap_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    user_transfer_authority_pubkey: &Pubkey,
    source_pubkey: &Pubkey,
    swap_source_pubkey: &Pubkey,
    swap_destination_pubkey: &Pubkey,
    destination_pubkey: &Pubkey,
    pool_mint_pubkey: &Pubkey,
    pool_fee_pubkey: &Pubkey,
    source_mint_pubkey: &Pubkey,
    destination_mint_pubkey: &Pubkey,
    swap_curve_pubkey: &Pubkey,
    host_fee_pubkey: Option<&Pubkey>,
    instruction: Swap,
) -> Result<Instruction, ProgramError> {
    let data = SwapInstruction::Swap {
        amount_in: instruction.amount_in,
        minimum_amount_out: instruction.minimum_amount_out,
    }
    .pack();

    let mut accounts = vec![
        AccountMeta::new_readonly(*swap_pubkey, false),
        AccountMeta::new_readonly(*authority_pubkey, false),
        AccountMeta::new_readonly(*user_transfer_authority_pubkey, true),
        AccountMeta::new(*source_pubkey, false),
        AccountMeta::new(*swap_source_pubkey, false),
        AccountMeta::new(*swap_destination_pubkey, false),
        AccountMeta::new(*destination_pubkey, false),
        AccountMeta::new(*pool_mint_pubkey, false),
        AccountMeta::new(*pool_fee_pubkey, false),
        AccountMeta::new_readonly(*source_mint_pubkey, false),
        AccountMeta::new_readonly(*destination_mint_pubkey, false),
        AccountMeta::new_readonly(*source_token_program_id, false),
        AccountMeta::new_readonly(*destination_token_program_id, false),
        AccountMeta::new_readonly(*pool_token_program_id, false),
        AccountMeta::new_readonly(*swap_curve_pubkey, false),
    ];
    if let Some(host_fee_pubkey) = host_fee_pubkey {
        accounts.push(AccountMeta::new(*host_fee_pubkey, false));
    }

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

pub fn dispatch_sig(namespace: &str, name: &str) -> [u8; 8] {
    let preimage = format!("{namespace}:{name}");

    let mut sighash = [0; 8];
    let mut hasher = Sha256::new();
    hasher.update(preimage.as_bytes());
    sighash.copy_from_slice(&hasher.finalize()[..8]);
    sighash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_swap() {
        let amount_in: u64 = 2;
        let minimum_amount_out: u64 = 10;
        let check = SwapInstruction::Swap {
            amount_in,
            minimum_amount_out,
        };
        let packed = check.pack();
        let mut expect = vec![1];
        expect.extend_from_slice(&amount_in.to_le_bytes());
        expect.extend_from_slice(&minimum_amount_out.to_le_bytes());
        assert_eq!(packed, expect);
        let unpacked = SwapInstruction::unpack(&expect).unwrap();
        assert_eq!(unpacked, check);
    }

    #[test]
    fn pack_deposit() {
        let pool_token_amount: u64 = 5;
        let maximum_token_a_amount: u64 = 10;
        let maximum_token_b_amount: u64 = 20;
        let check = SwapInstruction::DepositAllTokenTypes {
            pool_token_amount,
            maximum_token_a_amount,
            maximum_token_b_amount,
        };
        let packed = check.pack();
        let mut expect = vec![2];
        expect.extend_from_slice(&pool_token_amount.to_le_bytes());
        expect.extend_from_slice(&maximum_token_a_amount.to_le_bytes());
        expect.extend_from_slice(&maximum_token_b_amount.to_le_bytes());
        assert_eq!(packed, expect);
        let unpacked = SwapInstruction::unpack(&expect).unwrap();
        assert_eq!(unpacked, check);
    }

    #[test]
    fn pack_withdraw() {
        let pool_token_amount: u64 = 1212438012089;
        let minimum_token_a_amount: u64 = 102198761982612;
        let minimum_token_b_amount: u64 = 2011239855213;
        let check = SwapInstruction::WithdrawAllTokenTypes {
            pool_token_amount,
            minimum_token_a_amount,
            minimum_token_b_amount,
        };
        let packed = check.pack();
        let mut expect = vec![3];
        expect.extend_from_slice(&pool_token_amount.to_le_bytes());
        expect.extend_from_slice(&minimum_token_a_amount.to_le_bytes());
        expect.extend_from_slice(&minimum_token_b_amount.to_le_bytes());
        assert_eq!(packed, expect);
        let unpacked = SwapInstruction::unpack(&expect).unwrap();
        assert_eq!(unpacked, check);
    }

    #[test]
    fn pack_deposit_one_exact_in() {
        let source_token_amount: u64 = 10;
        let minimum_pool_token_amount: u64 = 5;
        let check = SwapInstruction::DepositSingleTokenTypeExactAmountIn {
            source_token_amount,
            minimum_pool_token_amount,
        };
        let packed = check.pack();
        let mut expect = vec![4];
        expect.extend_from_slice(&source_token_amount.to_le_bytes());
        expect.extend_from_slice(&minimum_pool_token_amount.to_le_bytes());
        assert_eq!(packed, expect);
        let unpacked = SwapInstruction::unpack(&expect).unwrap();
        assert_eq!(unpacked, check);
    }

    #[test]
    fn pack_withdraw_one_exact_out() {
        let destination_token_amount: u64 = 102198761982612;
        let maximum_pool_token_amount: u64 = 1212438012089;
        let check = SwapInstruction::WithdrawSingleTokenTypeExactAmountOut {
            destination_token_amount,
            maximum_pool_token_amount,
        };
        let packed = check.pack();
        let mut expect = vec![5];
        expect.extend_from_slice(&destination_token_amount.to_le_bytes());
        expect.extend_from_slice(&maximum_pool_token_amount.to_le_bytes());
        assert_eq!(packed, expect);
        let unpacked = SwapInstruction::unpack(&expect).unwrap();
        assert_eq!(unpacked, check);
    }
}
