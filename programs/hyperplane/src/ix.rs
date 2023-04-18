//! Instruction types

#![allow(clippy::too_many_arguments)]

use anchor_lang::{
    prelude::{Rent, System},
    solana_program::{
        instruction::Instruction, program_error::ProgramError, pubkey::Pubkey, sysvar::SysvarId,
    },
    Id, InstructionData, ToAccountMetas,
};
#[cfg(feature = "fuzz")]
use arbitrary::Arbitrary;
use derive_more::Constructor;

use crate::{
    curve::fees::Fees,
    instructions::CurveUserParameters,
    state::{UpdatePoolConfigMode, UpdatePoolConfigValue},
    InitialSupply,
};

/// Initialize instruction data
#[derive(Debug, PartialEq, Constructor)]
pub struct Initialize {
    /// all swap fees
    pub fees: Fees,
    /// swap curve info for pool, including CurveType and anything
    /// else that may be required
    pub curve_parameters: CurveUserParameters,
    /// initial supply of token A and B
    pub initial_supply: InitialSupply,
}

/// Swap instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq, Constructor)]
pub struct Swap {
    /// SOURCE amount to transfer, output to DESTINATION is based on the exchange rate
    pub amount_in: u64,
    /// Minimum amount of DESTINATION token to output, prevents excessive slippage
    pub minimum_amount_out: u64,
}

/// Deposit instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq, Constructor)]
pub struct Deposit {
    /// Pool token amount to transfer. token_a and token_b amount are set by
    /// the current exchange rate and size of the pool
    pub pool_token_amount: u64,
    /// Maximum token A amount to deposit, prevents excessive slippage
    pub maximum_token_a_amount: u64,
    /// Maximum token B amount to deposit, prevents excessive slippage
    pub maximum_token_b_amount: u64,
}

/// Withdraw instruction data
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq, Constructor)]
pub struct Withdraw {
    /// Amount of pool tokens to burn. User receives an output of token a
    /// and b based on the percentage of the pool tokens that are returned.
    pub pool_token_amount: u64,
    /// Minimum amount of token A to receive, prevents excessive slippage
    pub minimum_token_a_amount: u64,
    /// Minimum amount of token B to receive, prevents excessive slippage
    pub minimum_token_b_amount: u64,
}

/// WithdrawFees instruction data
#[derive(Clone, Debug, PartialEq, Constructor)]
pub struct WithdrawFees {
    /// Amount of trading tokens to withdraw
    pub requested_token_amount: u64,
}

/// UpdatePoolConfig instruction data
#[derive(Clone, Debug, PartialEq, Constructor)]
pub struct UpdatePoolConfig {
    /// Update mode
    pub mode: UpdatePoolConfigMode,
    /// Value to set
    pub value: UpdatePoolConfigValue,
}

impl From<UpdatePoolConfig> for crate::instruction::UpdatePoolConfig {
    fn from(value: UpdatePoolConfig) -> Self {
        crate::instruction::UpdatePoolConfig {
            mode: value.mode as u16,
            value: value.value.to_bytes(),
        }
    }
}

/// Creates an 'initialize' instruction.
pub fn initialize_pool(
    program_id: &Pubkey,
    admin: &Pubkey,
    pool: &Pubkey,
    swap_curve: &Pubkey,
    token_a_mint: &Pubkey,
    token_b_mint: &Pubkey,
    token_a_vault: &Pubkey,
    token_b_vault: &Pubkey,
    pool_authority: &Pubkey,
    pool_token_mint: &Pubkey,
    token_a_fees_vault: &Pubkey,
    token_b_fees_vault: &Pubkey,
    admin_token_a_ata: &Pubkey,
    admin_token_b_ata: &Pubkey,
    admin_pool_token_ata: &Pubkey,
    pool_token_program_id: &Pubkey,
    token_a_program_id: &Pubkey,
    token_b_program_id: &Pubkey,
    Initialize {
        fees,
        curve_parameters,
        initial_supply:
            InitialSupply {
                initial_supply_a,
                initial_supply_b,
            },
    }: Initialize,
) -> Result<Instruction, ProgramError> {
    let data = super::instruction::InitializePool {
        initial_supply_a,
        initial_supply_b,
        fees,
        curve_parameters,
    }
    .data();

    let accounts = super::accounts::InitializePool {
        admin: *admin,
        pool: *pool,
        swap_curve: *swap_curve,
        pool_authority: *pool_authority,
        token_a_mint: *token_a_mint,
        token_b_mint: *token_b_mint,
        token_a_vault: *token_a_vault,
        token_b_vault: *token_b_vault,
        pool_token_mint: *pool_token_mint,
        token_a_fees_vault: *token_a_fees_vault,
        token_b_fees_vault: *token_b_fees_vault,
        admin_token_a_ata: *admin_token_a_ata,
        admin_token_b_ata: *admin_token_b_ata,
        admin_pool_token_ata: *admin_pool_token_ata,
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

/// Creates a 'deposit' instruction.
pub fn deposit(
    program_id: &Pubkey,
    user_transfer_authority_pubkey: &Pubkey,
    pool: &Pubkey,
    swap_curve: &Pubkey,
    pool_authority: &Pubkey,
    token_a_mint: &Pubkey,
    token_b_mint: &Pubkey,
    token_a_vault: &Pubkey,
    token_b_vault: &Pubkey,
    pool_token_mint: &Pubkey,
    user_token_a_ata: &Pubkey,
    user_token_b_ata: &Pubkey,
    user_pool_token_ata: &Pubkey,
    pool_token_program: &Pubkey,
    token_a_program: &Pubkey,
    token_b_program: &Pubkey,
    Deposit {
        pool_token_amount,
        maximum_token_a_amount,
        maximum_token_b_amount,
    }: Deposit,
) -> Result<Instruction, ProgramError> {
    let data = super::instruction::Deposit {
        pool_token_amount,
        maximum_token_a_amount,
        maximum_token_b_amount,
    }
    .data();

    let accounts = super::accounts::Deposit {
        signer: *user_transfer_authority_pubkey,
        pool: *pool,
        swap_curve: *swap_curve,
        pool_authority: *pool_authority,
        token_a_mint: *token_a_mint,
        token_b_mint: *token_b_mint,
        token_a_vault: *token_a_vault,
        token_b_vault: *token_b_vault,
        pool_token_mint: *pool_token_mint,
        token_a_user_ata: *user_token_a_ata,
        token_b_user_ata: *user_token_b_ata,
        pool_token_user_ata: *user_pool_token_ata,
        pool_token_program: *pool_token_program,
        token_a_token_program: *token_a_program,
        token_b_token_program: *token_b_program,
    }
    .to_account_metas(None);

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'withdraw' instruction.
pub fn withdraw(
    program_id: &Pubkey,
    user_transfer_authority: &Pubkey,
    pool: &Pubkey,
    swap_curve: &Pubkey,
    pool_authority: &Pubkey,
    token_a_mint: &Pubkey,
    token_b_mint: &Pubkey,
    token_a_vault: &Pubkey,
    token_b_vault: &Pubkey,
    pool_token_mint: &Pubkey,
    token_a_fees_vault: &Pubkey,
    token_b_fees_vault: &Pubkey,
    user_token_a_ata: &Pubkey,
    user_token_b_ata: &Pubkey,
    user_pool_token_ata: &Pubkey,
    pool_token_program: &Pubkey,
    token_a_program: &Pubkey,
    token_b_program: &Pubkey,
    Withdraw {
        pool_token_amount,
        minimum_token_a_amount,
        minimum_token_b_amount,
    }: Withdraw,
) -> Result<Instruction, ProgramError> {
    let data = super::instruction::Withdraw {
        pool_token_amount,
        minimum_token_a_amount,
        minimum_token_b_amount,
    }
    .data();

    let accounts = super::accounts::Withdraw {
        signer: *user_transfer_authority,
        pool: *pool,
        swap_curve: *swap_curve,
        pool_authority: *pool_authority,
        token_a_mint: *token_a_mint,
        token_b_mint: *token_b_mint,
        token_a_vault: *token_a_vault,
        token_b_vault: *token_b_vault,
        pool_token_mint: *pool_token_mint,
        token_a_fees_vault: *token_a_fees_vault,
        token_b_fees_vault: *token_b_fees_vault,
        token_a_user_ata: *user_token_a_ata,
        token_b_user_ata: *user_token_b_ata,
        pool_token_user_ata: *user_pool_token_ata,
        pool_token_program: *pool_token_program,
        token_a_token_program: *token_a_program,
        token_b_token_program: *token_b_program,
    }
    .to_account_metas(None);

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'swap' instruction.
pub fn swap(
    program_id: &Pubkey,
    user_transfer_authority: &Pubkey,
    pool: &Pubkey,
    swap_curve: &Pubkey,
    pool_authority: &Pubkey,
    source_mint: &Pubkey,
    destination_mint: &Pubkey,
    source_vault: &Pubkey,
    destination_vault: &Pubkey,
    source_token_fees_vault: &Pubkey,
    source_user_ata: &Pubkey,
    destination_user_ata: &Pubkey,
    source_token_host_fees: Option<&Pubkey>,
    source_token_program_id: &Pubkey,
    destination_token_program_id: &Pubkey,
    Swap {
        amount_in,
        minimum_amount_out,
    }: Swap,
) -> Result<Instruction, ProgramError> {
    let data = super::instruction::Swap {
        amount_in,
        minimum_amount_out,
    }
    .data();

    let accounts = super::accounts::Swap {
        signer: *user_transfer_authority,
        pool: *pool,
        swap_curve: *swap_curve,
        pool_authority: *pool_authority,
        source_mint: *source_mint,
        destination_mint: *destination_mint,
        source_vault: *source_vault,
        destination_vault: *destination_vault,
        source_token_fees_vault: *source_token_fees_vault,
        source_user_ata: *source_user_ata,
        destination_user_ata: *destination_user_ata,
        source_token_host_fees_account: source_token_host_fees.copied(),
        source_token_program: *source_token_program_id,
        destination_token_program: *destination_token_program_id,
    }
    .to_account_metas(None);

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates a 'withdraw_fees' instruction.
pub fn withdraw_fees(
    program_id: &Pubkey,
    admin: &Pubkey,
    pool: &Pubkey,
    pool_authority: &Pubkey,
    fees_mint: &Pubkey,
    fees_vault: &Pubkey,
    admin_fees_ata: &Pubkey,
    fees_token_program: &Pubkey,
    WithdrawFees {
        requested_token_amount: requested_pool_token_amount,
    }: WithdrawFees,
) -> Result<Instruction, ProgramError> {
    let data = super::instruction::WithdrawFees {
        requested_pool_token_amount,
    }
    .data();

    let accounts = super::accounts::WithdrawFees {
        admin: *admin,
        pool: *pool,
        pool_authority: *pool_authority,
        fees_mint: *fees_mint,
        fees_vault: *fees_vault,
        admin_fees_ata: *admin_fees_ata,
        fees_token_program: *fees_token_program,
    }
    .to_account_metas(None);

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}

/// Creates an 'update pool config' instruction.
pub fn update_pool_config(
    program_id: &Pubkey,
    admin: &Pubkey,
    pool: &Pubkey,
    UpdatePoolConfig { mode, value }: UpdatePoolConfig,
) -> Result<Instruction, ProgramError> {
    let data = super::instruction::UpdatePoolConfig {
        mode: mode as u16,
        value: value.to_bytes(),
    }
    .data();

    let accounts = super::accounts::UpdatePoolConfig {
        admin: *admin,
        pool: *pool,
    }
    .to_account_metas(None);

    Ok(Instruction {
        program_id: *program_id,
        accounts,
        data,
    })
}
