use crate::curve::base::SwapCurve;
use crate::curve::calculator::{RoundDirection, TradeDirection};
use crate::{curve, emitted, event, require_msg, to_u64};
use anchor_lang::accounts::interface::Interface;
use anchor_lang::accounts::interface_account::InterfaceAccount;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::transfer_fee::TransferFeeConfig;
use anchor_spl::token_2022::spl_token_2022::extension::{
    BaseStateWithExtensions, StateWithExtensions,
};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::error::SwapError;
use crate::state::SwapPool;
use crate::state::SwapState;
use crate::swap::utils::validate_swap_inputs;
use crate::utils::{pool_token, swap_token};

pub fn handler(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<event::Swap> {
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
    let actual_amount_in =
        utils::sub_transfer_fee(&ctx.accounts.source_mint.to_account_info(), amount_in)?;

    msg!(
        "Swap pool inputs: swap_type={:?}, source_token_balance={}, destination_token_balance={}, pool_token_supply={}",
        swap_curve.curve_type,
        ctx.accounts.source_vault.amount,
        ctx.accounts.destination_vault.amount,
        ctx.accounts.pool_token_mint.supply,
    );
    let result = swap_curve
        .swap(
            u128::from(actual_amount_in),
            u128::from(ctx.accounts.source_vault.amount),
            u128::from(ctx.accounts.destination_vault.amount),
            trade_direction,
            pool.fees(),
        )
        .map_err(|_| error!(SwapError::ZeroTradingTokens))?;

    // Re-calculate the source amount swapped based on what the curve says
    let source_amount_swapped = to_u64!(result.source_amount_swapped)?;
    let source_transfer_amount = utils::add_transfer_fee(
        &ctx.accounts.source_mint.to_account_info(),
        source_amount_swapped,
    )?;

    let destination_transfer_amount = to_u64!(result.destination_amount_swapped)?;
    let amount_received = utils::sub_transfer_fee(
        &ctx.accounts.destination_mint.to_account_info(),
        destination_transfer_amount,
    )?;
    require_msg!(
        amount_received >= minimum_amount_out,
        SwapError::ExceededSlippage,
        &format!(
            "ExceededSlippage: amount_received={} < minimum_amount_out={}",
            amount_received, minimum_amount_out
        )
    );

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
        ctx.accounts.source_mint.decimals,
    )?;

    if result.owner_fee > 0 {
        let mut pool_token_amount = swap_curve
            .calculator
            .withdraw_single_token_type_exact_out(
                result.owner_fee,
                swap_token_a_amount,
                swap_token_b_amount,
                u128::from(ctx.accounts.pool_token_mint.supply),
                trade_direction,
                RoundDirection::Floor,
            )
            .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
        // Allow error to fall through
        // todo - elliot - optional front-end host fees
        if let Some(host_fees_account) = &ctx.accounts.pool_token_host_fees_account {
            let host_fee = pool
                .fees()
                .host_fee(pool_token_amount)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
            if host_fee > 0 {
                pool_token_amount = pool_token_amount
                    .checked_sub(host_fee)
                    .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;

                pool_token::mint(
                    ctx.accounts.pool_token_program.to_account_info(),
                    ctx.accounts.pool.to_account_info(),
                    ctx.accounts.pool_token_mint.to_account_info(),
                    ctx.accounts.pool_authority.to_account_info(),
                    pool.pool_authority_bump_seed,
                    host_fees_account.to_account_info(),
                    to_u64!(host_fee)?,
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
            to_u64!(pool_token_amount)?,
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
        ctx.accounts.destination_mint.decimals,
    )?;

    let fee = result
        .owner_fee
        .checked_add(result.trade_fee)
        .ok_or_else(|| error!(SwapError::CalculationFailure))?;

    emitted!(event::Swap {
        token_in_amount: source_transfer_amount,
        token_out_amount: destination_transfer_amount,
        fee: to_u64!(fee)?,
    });
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
    pub source_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: checked in the handler
    // note - constraint repeated for clarity
    #[account(
        constraint = source_mint.key() != destination_mint.key() @ SwapError::RepeatedMint,
    )]
    pub destination_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: checked in the handler
    #[account(mut)]
    pub source_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: checked in the handler
    #[account(mut)]
    pub destination_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Account to collect fees into
    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_fees_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Signer's source token account
    // note - authority constraint repeated for clarity
    #[account(mut,
        token::mint = source_mint,
        token::authority = destination_user_ata.owner,
        token::token_program = source_token_program,
    )]
    pub source_user_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Signer's destination token account
    // note - authority constraint repeated for clarity
    #[account(mut,
        token::mint = destination_mint,
        token::authority = source_user_ata.owner,
        token::token_program = destination_token_program,
    )]
    pub destination_user_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    // todo - elliot - probably remove this - user can add their own account to get a better deal
    /// Optional pool token fees account for front ends - if not present, all fees are sent to the pool fees account
    #[account(mut,
        token::mint = pool_token_mint,
        token::token_program = pool_token_program,
    )]
    pub pool_token_host_fees_account: Option<Box<InterfaceAccount<'info, TokenAccount>>>,

    /// Token program for the pool token mint
    pub pool_token_program: Interface<'info, TokenInterface>,
    /// Token program for the source mint
    pub source_token_program: Interface<'info, TokenInterface>,
    /// Token program for the destination mint
    pub destination_token_program: Interface<'info, TokenInterface>,
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
                require_msg!(
                    ctx.accounts.source_vault.key() == pool.token_a_vault,
                    SwapError::IncorrectSwapAccount,
                    &format!(
                        "IncorrectSwapAccount: source_vault.key ({}) != token_a_vault.key ({})",
                        ctx.accounts.source_vault.key(),
                        pool.token_a_vault.key()
                    )
                );
                require_msg!(
                    ctx.accounts.destination_vault.key() == pool.token_b_vault,
                    SwapError::IncorrectSwapAccount,
                    &format!(
                        "IncorrectSwapAccount: destination_vault.key ({}) != token_b_vault.key ({})",
                        ctx.accounts.destination_vault.key(),
                        pool.token_b_vault.key()
                    )
                );
            }
            TradeDirection::BtoA => {
                require_msg!(
                    ctx.accounts.destination_vault.key() == pool.token_a_vault,
                    SwapError::IncorrectSwapAccount,
                    &format!(
                        "IncorrectSwapAccount: destination_vault.key ({}) != token_a_vault.key ({})",
                        ctx.accounts.source_vault.key(),
                        pool.token_a_vault.key()
                    )
                );
                require_msg!(
                    ctx.accounts.source_vault.key() == pool.token_b_vault,
                    SwapError::IncorrectSwapAccount,
                    &format!(
                        "IncorrectSwapAccount: source_vault.key ({}) != token_b_vault.key ({})",
                        ctx.accounts.source_vault.key(),
                        pool.token_b_vault.key()
                    )
                );
            }
        };

        Ok(trade_direction)
    }

    /// Subtract token mint transfer fees for actual amount transferred
    pub fn sub_transfer_fee(mint_acc_info: &AccountInfo, amount: u64) -> Result<u64> {
        let mint_data = mint_acc_info.data.borrow();
        let mint =
            StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
                &mint_data,
            )?;
        let amount = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
            let transfer_fee = transfer_fee_config
                .calculate_epoch_fee(Clock::get()?.epoch, amount)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
            let amount_sub_fee = amount.saturating_sub(transfer_fee);
            msg!(
                "Subtract token transfer fee: fee={}, amount={}, amount_sub_fee={}",
                transfer_fee,
                amount,
                amount_sub_fee
            );
            amount_sub_fee
        } else {
            amount
        };
        Ok(amount)
    }

    /// Add token mint transfer fees for actual amount transferred
    pub fn add_transfer_fee(mint_acc_info: &AccountInfo, amount: u64) -> Result<u64> {
        let mint_data = mint_acc_info.data.borrow();
        let mint =
            StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
                &mint_data,
            )?;
        let amount = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
            let transfer_fee = transfer_fee_config
                .calculate_inverse_epoch_fee(Clock::get()?.epoch, amount)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
            let amount_add_fee = amount.saturating_add(transfer_fee);
            msg!(
                "Add token transfer fee: fee={}, amount={}, amount_add_fee={}",
                transfer_fee,
                amount,
                amount_add_fee
            );
            amount_add_fee
        } else {
            amount
        };
        Ok(amount)
    }
}
