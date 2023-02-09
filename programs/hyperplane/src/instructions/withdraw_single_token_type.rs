use crate::curve::base::SwapCurve;
use crate::curve::calculator::TradeDirection;
use crate::{curve, emitted, event, require_msg, to_u64};
use anchor_lang::accounts::compatible_program::CompatibleProgram;
use anchor_lang::accounts::multi_program_compatible_account::MultiProgramCompatibleAccount;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::{Mint, Token, TokenAccount};

use crate::error::SwapError;
use crate::state::SwapPool;
use crate::state::SwapState;
use crate::utils::{pool_token, swap_token};
use crate::withdraw_single_token_type::utils::validate_swap_inputs;

pub fn handler(
    ctx: Context<WithdrawSingleTokenType>,
    destination_token_amount: u64,
    maximum_pool_token_amount: u64,
) -> Result<event::WithdrawSingleTokenType> {
    let trade_direction = validate_swap_inputs(&ctx)?;
    let pool = ctx.accounts.pool.load()?;
    msg!(
        "Withdraw inputs: destination_token_amount={}, maximum_pool_token_amount={}",
        destination_token_amount,
        maximum_pool_token_amount,
    );
    let swap_curve = curve!(ctx.accounts.swap_curve, pool);

    msg!(
        "Swap pool inputs: swap_type={:?}, token_a_balance={}, token_b_balance={}, pool_token_supply={}",
        swap_curve.curve_type,
        ctx.accounts.token_a_vault.amount,
        ctx.accounts.token_b_vault.amount,
        ctx.accounts.pool_token_mint.supply,
    );

    let pool_mint_supply = u128::from(ctx.accounts.pool_token_mint.supply);
    let burn_pool_token_amount = swap_curve
        .withdraw_single_token_type_exact_out(
            u128::from(destination_token_amount),
            u128::from(ctx.accounts.token_a_vault.amount),
            u128::from(ctx.accounts.token_b_vault.amount),
            pool_mint_supply,
            trade_direction,
            pool.fees(),
        )
        .ok_or(SwapError::ZeroTradingTokens)?;

    let withdraw_fee = pool
        .fees()
        .owner_withdraw_fee(burn_pool_token_amount)
        .ok_or(SwapError::FeeCalculationFailure)?;
    let pool_token_amount = burn_pool_token_amount
        .checked_add(withdraw_fee)
        .ok_or(SwapError::CalculationFailure)?;

    msg!(
        "Withdrawal fee: fee={}, amount_after_fee={}",
        withdraw_fee,
        pool_token_amount
    );

    require_msg!(
        pool_token_amount <= maximum_pool_token_amount.into(),
        SwapError::ExceededSlippage,
        &format!(
            "ExceededSlippage: pool_token_amount={} > maximum_pool_token_amount={}",
            pool_token_amount, maximum_pool_token_amount
        )
    );
    require!(pool_token_amount > 0, SwapError::ZeroTradingTokens);

    let withdraw_fee = to_u64!(withdraw_fee)?;
    if withdraw_fee > 0 {
        swap_token::transfer_from_user(
            ctx.accounts.pool_token_program.to_account_info(),
            ctx.accounts.pool_token_user_ata.to_account_info(),
            ctx.accounts.pool_token_mint.to_account_info(),
            ctx.accounts.pool_token_fees_vault.to_account_info(),
            ctx.accounts.signer.to_account_info(),
            withdraw_fee,
            ctx.accounts.pool_token_mint.decimals,
        )?;
    }

    msg!(
        "Withdraw outputs: destination_token_amount={}, pool_tokens_to_burn={}",
        destination_token_amount,
        burn_pool_token_amount,
    );

    pool_token::burn(
        ctx.accounts.pool_token_mint.to_account_info(),
        ctx.accounts.pool_token_user_ata.to_account_info(),
        ctx.accounts.signer.to_account_info(),
        ctx.accounts.pool_token_program.to_account_info(),
        to_u64!(burn_pool_token_amount)?,
    )?;

    let destination_vault = match trade_direction {
        TradeDirection::AtoB => &ctx.accounts.token_a_vault,
        TradeDirection::BtoA => &ctx.accounts.token_b_vault,
    };
    swap_token::transfer_from_vault(
        ctx.accounts.destination_token_program.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        destination_vault.to_account_info(),
        ctx.accounts.destination_token_mint.to_account_info(),
        ctx.accounts.destination_token_user_ata.to_account_info(),
        ctx.accounts.pool_authority.to_account_info(),
        pool.pool_authority_bump_seed,
        destination_token_amount,
        ctx.accounts.destination_token_mint.decimals,
    )?;

    emitted!(event::WithdrawSingleTokenType {
        pool_token_amount: to_u64!(pool_token_amount)?,
        token_amount: destination_token_amount,
        fee: withdraw_fee,
    });
}

#[derive(Accounts)]
pub struct WithdrawSingleTokenType<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut,
        has_one = swap_curve,
        has_one = pool_authority @ SwapError::InvalidProgramAddress,
        has_one = token_a_vault @ SwapError::IncorrectSwapAccount,
        has_one = token_b_vault @ SwapError::IncorrectSwapAccount,
        has_one = pool_token_mint @ SwapError::IncorrectPoolMint,
        has_one = pool_token_fees_vault @ SwapError::IncorrectFeeAccount,
    )]
    pub pool: AccountLoader<'info, SwapPool>,

    /// CHECK: has_one constraint on the pool
    pub swap_curve: UncheckedAccount<'info>,

    /// CHECK: has_one constraint on the pool
    pub pool_authority: AccountInfo<'info>,

    /// CHECK: checked in the handler
    pub destination_token_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub token_a_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub token_b_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// Account to collect fees into
    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_fees_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// Signer's token B token account
    #[account(mut,
        token::mint = destination_token_mint,
        token::authority = signer,
        token::token_program = destination_token_program,
    )]
    pub destination_token_user_ata: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// Signer's pool token account
    #[account(mut,
        token::mint = pool_token_mint,
        token::authority = signer,
        token::token_program = pool_token_program,
    )]
    pub pool_token_user_ata: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// Token program for the pool token mint
    pub pool_token_program: CompatibleProgram<'info, Token>,
    /// Token program for the source mint
    pub destination_token_program: CompatibleProgram<'info, Token>,
}

mod utils {
    use super::*;

    pub fn validate_swap_inputs(ctx: &Context<WithdrawSingleTokenType>) -> Result<TradeDirection> {
        let trade_direction = if ctx.accounts.destination_token_user_ata.mint
            == ctx.accounts.token_a_vault.mint
        {
            TradeDirection::AtoB
        } else if ctx.accounts.destination_token_user_ata.mint == ctx.accounts.token_b_vault.mint {
            TradeDirection::BtoA
        } else {
            msg!("IncorrectSwapAccount: destination_token_user_ata.mint ({}) != token_a_vault.mint ({}) || token_b_vault.mint ({})", ctx.accounts.destination_token_user_ata.mint, ctx.accounts.token_a_vault.mint, ctx.accounts.token_b_vault.mint);
            return err!(SwapError::IncorrectSwapAccount);
        };

        Ok(trade_direction)
    }
}
