use crate::curve;
use crate::curve::base::SwapCurve;
use crate::curve::calculator::{RoundDirection, TradeDirection};
use crate::utils::math::{to_u128, to_u64};
use anchor_lang::accounts::compatible_program::CompatibleProgram;
use anchor_lang::accounts::multi_program_compatible_account::MultiProgramCompatibleAccount;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::transfer_fee::TransferFeeConfig;
use anchor_spl::token_2022::spl_token_2022::extension::{
    BaseStateWithExtensions, StateWithExtensions,
};
use anchor_spl::token_2022::{Mint, Token, TokenAccount};
use std::ops::Deref;

use crate::error::SwapError;
use crate::state::SwapPool;
use crate::state::SwapState;
use crate::swap::utils::validate_swap_inputs;
use crate::utils::{pool_token, swap_token};

pub fn handler(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<()> {
    let pool = ctx.accounts.pool.load()?;
    let trade_direction = validate_swap_inputs(&ctx, &pool)?;
    msg!(
        "Swap inputs: trade_direction={:?}, amount_in={}, minimum_amount_out={}",
        trade_direction,
        amount_in,
        minimum_amount_out
    );
    let swap_curve = curve!(ctx.accounts.swap_curve, pool);

    // Take transfer fees into account for actual amount transferred in
    let actual_amount_in = {
        let source_mint_acc_info = ctx.accounts.source_mint.to_account_info();
        let source_mint_data = source_mint_acc_info.data.borrow();
        let source_mint =
            StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
                source_mint_data.deref(),
            )?;

        if let Ok(transfer_fee_config) = source_mint.get_extension::<TransferFeeConfig>() {
            let transfer_fee = transfer_fee_config
                .calculate_epoch_fee(Clock::get()?.epoch, amount_in)
                .ok_or(SwapError::FeeCalculationFailure)?;
            let amount_in_after_fee = amount_in.saturating_sub(transfer_fee);
            msg!(
                "Subtracted input token transfer fee: fee={}, amount_in_after_fee={}",
                transfer_fee,
                amount_in_after_fee
            );
            amount_in_after_fee
        } else {
            amount_in
        }
    };

    msg!(
        "Swap pool inputs: swap_type={:?}, source_token_balance={}, destination_token_balance={}, pool_token_supply={}",
        swap_curve.curve_type,
        ctx.accounts.source_vault.amount,
        ctx.accounts.destination_vault.amount,
        ctx.accounts.pool_token_mint.supply,
    );
    let result = swap_curve
        .swap(
            to_u128(actual_amount_in)?,
            to_u128(ctx.accounts.source_vault.amount)?,
            to_u128(ctx.accounts.destination_vault.amount)?,
            trade_direction,
            pool.fees(),
        )
        .ok_or(SwapError::ZeroTradingTokens)?;

    // Re-calculate the source amount swapped based on what the curve says
    let (source_transfer_amount, source_mint_decimals) = {
        let source_amount_swapped = to_u64(result.source_amount_swapped)?;

        let source_mint_acc_info = ctx.accounts.source_mint.to_account_info();
        let source_mint_data = source_mint_acc_info.data.borrow();
        let source_mint =
            StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
                source_mint_data.deref(),
            )?;

        let amount =
            if let Ok(transfer_fee_config) = source_mint.get_extension::<TransferFeeConfig>() {
                source_amount_swapped.saturating_add(
                    transfer_fee_config
                        .calculate_inverse_epoch_fee(Clock::get()?.epoch, source_amount_swapped)
                        .ok_or(SwapError::FeeCalculationFailure)?,
                )
            } else {
                source_amount_swapped
            };
        (amount, source_mint.base.decimals)
    };

    let (destination_transfer_amount, destination_mint_decimals) = {
        let destination_mint_acc_info = ctx.accounts.destination_mint.to_account_info();
        let destination_mint_data = destination_mint_acc_info.data.borrow();
        let destination_mint = StateWithExtensions::<
            anchor_spl::token_2022::spl_token_2022::state::Mint,
        >::unpack(destination_mint_data.deref())?;

        let amount_out = to_u64(result.destination_amount_swapped)?;
        let amount_received = if let Ok(transfer_fee_config) =
            destination_mint.get_extension::<TransferFeeConfig>()
        {
            amount_out.saturating_sub(
                transfer_fee_config
                    .calculate_epoch_fee(Clock::get()?.epoch, amount_out)
                    .ok_or(SwapError::FeeCalculationFailure)?,
            )
        } else {
            amount_out
        };
        if amount_received < minimum_amount_out {
            return Err(SwapError::ExceededSlippage.into());
        }
        (amount_out, destination_mint.base.decimals)
    };

    let (swap_token_a_amount, swap_token_b_amount) = match trade_direction {
        TradeDirection::AtoB => (
            result.new_swap_source_amount,
            result.new_swap_destination_amount,
        ),
        TradeDirection::BtoA => (
            result.new_swap_destination_amount,
            result.new_swap_source_amount,
        ),
    };

    swap_token::transfer_from_user(
        ctx.accounts.source_token_program.to_account_info(),
        ctx.accounts.source_user_ata.to_account_info(),
        ctx.accounts.source_mint.to_account_info(),
        ctx.accounts.source_vault.to_account_info(),
        ctx.accounts.signer.to_account_info(),
        source_transfer_amount,
        source_mint_decimals,
    )?;

    if result.owner_fee > 0 {
        let mut pool_token_amount = swap_curve
            .calculator
            .withdraw_single_token_type_exact_out(
                result.owner_fee,
                swap_token_a_amount,
                swap_token_b_amount,
                to_u128(ctx.accounts.pool_token_mint.supply)?,
                trade_direction,
                RoundDirection::Floor,
            )
            .ok_or(SwapError::FeeCalculationFailure)?;
        // Allow error to fall through
        // todo - elliot - optional front-end host fees
        if let Some(host_fees_account) = &ctx.accounts.pool_token_host_fees_account {
            let host_fee = pool
                .fees()
                .host_fee(pool_token_amount)
                .ok_or(SwapError::FeeCalculationFailure)?;
            if host_fee > 0 {
                pool_token_amount = pool_token_amount
                    .checked_sub(host_fee)
                    .ok_or(SwapError::FeeCalculationFailure)?;

                pool_token::mint(
                    ctx.accounts.pool_token_program.to_account_info(),
                    ctx.accounts.pool.to_account_info(),
                    ctx.accounts.pool_token_mint.to_account_info(),
                    ctx.accounts.pool_authority.to_account_info(),
                    pool.pool_authority_bump_seed,
                    host_fees_account.to_account_info(),
                    to_u64(host_fee)?,
                )?;
            }
        }
        pool_token::mint(
            ctx.accounts.pool_token_program.to_account_info(),
            ctx.accounts.pool.to_account_info(),
            ctx.accounts.pool_token_mint.to_account_info(),
            ctx.accounts.pool_authority.to_account_info(),
            pool.pool_authority_bump_seed,
            ctx.accounts.pool_token_fees_vault.to_account_info(),
            to_u64(pool_token_amount)?,
        )?;
    }

    swap_token::transfer_from_vault(
        ctx.accounts.destination_token_program.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.destination_vault.to_account_info(),
        ctx.accounts.destination_mint.to_account_info(),
        ctx.accounts.destination_user_ata.to_account_info(),
        ctx.accounts.pool_authority.to_account_info(),
        pool.pool_authority_bump_seed,
        destination_transfer_amount,
        destination_mint_decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut,
        has_one = swap_curve,
        has_one = pool_authority @ SwapError::InvalidProgramAddress,
        has_one = pool_token_mint @ SwapError::IncorrectPoolMint,
        has_one = pool_token_fees_vault @ SwapError::IncorrectFeeAccount,
    )]
    pub pool: AccountLoader<'info, SwapPool>,

    /// CHECK: has_one constraint on the pool
    pub swap_curve: UncheckedAccount<'info>,

    /// CHECK: has_one constraint on the pool
    pub pool_authority: AccountInfo<'info>,

    /// CHECK: checked in the handler
    // note - constraint repeated for clarity
    #[account(
        constraint = source_mint.key() != destination_mint.key() @ SwapError::RepeatedMint,
    )]
    pub source_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// CHECK: checked in the handler
    // note - constraint repeated for clarity
    #[account(
        constraint = source_mint.key() != destination_mint.key() @ SwapError::RepeatedMint,
    )]
    pub destination_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// CHECK: checked in the handler
    #[account(mut)]
    pub source_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// CHECK: checked in the handler
    #[account(mut)]
    pub destination_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// Account to collect fees into
    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_fees_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// Signer's source token account
    #[account(mut,
        token::mint = source_mint,
        token::authority = signer,
        token::token_program = source_token_program,
    )]
    pub source_user_ata: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// Signer's destination token account
    #[account(mut,
        token::mint = destination_mint,
        token::authority = signer,
        token::token_program = destination_token_program,
    )]
    pub destination_user_ata: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    // todo - elliot - probably remove this - user can add their own account to get a better deal
    /// Optional pool token fees account for front ends - if not present, all fees are sent to the pool fees account
    #[account(mut,
        token::mint = pool_token_mint,
        token::token_program = pool_token_program,
    )]
    pub pool_token_host_fees_account:
        Option<Box<MultiProgramCompatibleAccount<'info, TokenAccount>>>,

    /// Token program for the pool token mint
    pub pool_token_program: CompatibleProgram<'info, Token>,
    /// Token program for the source mint
    pub source_token_program: CompatibleProgram<'info, Token>,
    /// Token program for the destination mint
    pub destination_token_program: CompatibleProgram<'info, Token>,
}

mod utils {
    use super::*;
    use std::cell::Ref;

    pub fn validate_swap_inputs(
        ctx: &Context<Swap>,
        pool: &Ref<SwapPool>,
    ) -> Result<TradeDirection> {
        let trade_direction = if ctx.accounts.source_mint.key() == pool.token_a_mint
            && ctx.accounts.destination_mint.key() == pool.token_b_mint
        {
            TradeDirection::AtoB
        } else if ctx.accounts.source_mint.key() == pool.token_b_mint
            && ctx.accounts.destination_mint.key() == pool.token_a_mint
        {
            TradeDirection::BtoA
        } else {
            return err!(SwapError::IncorrectSwapAccount);
        };

        match trade_direction {
            TradeDirection::AtoB => {
                if ctx.accounts.source_vault.key() != pool.token_a_vault
                    || ctx.accounts.destination_vault.key() != pool.token_b_vault
                {
                    return err!(SwapError::IncorrectSwapAccount);
                }
            }
            TradeDirection::BtoA => {
                if ctx.accounts.source_vault.key() != pool.token_b_vault
                    || ctx.accounts.destination_vault.key() != pool.token_a_vault
                {
                    return err!(SwapError::IncorrectSwapAccount);
                }
            }
        };

        Ok(trade_direction)
    }
}
