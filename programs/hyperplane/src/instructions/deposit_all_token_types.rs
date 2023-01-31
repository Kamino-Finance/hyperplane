use crate::curve;
use crate::curve::base::SwapCurve;
use crate::curve::calculator::RoundDirection;
use crate::utils::math::{to_u128, to_u64};
use anchor_lang::accounts::compatible_program::CompatibleProgram;
use anchor_lang::accounts::multi_program_compatible_account::MultiProgramCompatibleAccount;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::{Mint, Token, TokenAccount};

use crate::error::SwapError;
use crate::event::SwapEvent;
use crate::state::SwapPool;
use crate::state::SwapState;
use crate::utils::{pool_token, swap_token};

pub fn handler(
    ctx: Context<DepositAllTokenTypes>,
    pool_token_amount: u64,
    maximum_token_a_amount: u64,
    maximum_token_b_amount: u64,
) -> Result<SwapEvent> {
    let pool = ctx.accounts.pool.load()?;
    msg!(
        "Deposit inputs: maximum_token_a_amount={}, maximum_token_b_amount={}, pool_token_amount={}",
        maximum_token_a_amount,
        maximum_token_b_amount,
        pool_token_amount,
    );
    let swap_curve = curve!(ctx.accounts.swap_curve, pool);

    let calculator = &swap_curve.calculator;
    if !calculator.allows_deposits() {
        return Err(SwapError::UnsupportedCurveOperation.into());
    }

    msg!(
        "Swap pool inputs: swap_type={:?}, token_a_balance={}, token_b_balance={}, pool_token_supply={}",
        swap_curve.curve_type,
        ctx.accounts.token_a_vault.amount,
        ctx.accounts.token_b_vault.amount,
        ctx.accounts.pool_token_mint.supply,
    );

    let current_pool_mint_supply = to_u128(ctx.accounts.pool_token_mint.supply)?;
    let (pool_token_amount, pool_mint_supply) = if current_pool_mint_supply > 0 {
        (to_u128(pool_token_amount)?, current_pool_mint_supply)
    } else {
        (calculator.new_pool_supply(), calculator.new_pool_supply())
    };

    let results = calculator
        .pool_tokens_to_trading_tokens(
            pool_token_amount,
            pool_mint_supply,
            to_u128(ctx.accounts.token_a_vault.amount)?,
            to_u128(ctx.accounts.token_b_vault.amount)?,
            RoundDirection::Ceiling,
        )
        .ok_or(SwapError::ZeroTradingTokens)?;
    let token_a_amount = to_u64(results.token_a_amount)?;
    if token_a_amount > maximum_token_a_amount {
        msg!(
            "ExceededSlippage: token_a_amount={} > maximum_token_a_amount={}",
            token_a_amount,
            maximum_token_a_amount
        );
        return Err(SwapError::ExceededSlippage.into());
    }
    if token_a_amount == 0 {
        return Err(SwapError::ZeroTradingTokens.into());
    }
    let token_b_amount = to_u64(results.token_b_amount)?;
    if token_b_amount > maximum_token_b_amount {
        msg!(
            "ExceededSlippage: token_b_amount={} > maximum_token_b_amount={}",
            token_b_amount,
            maximum_token_b_amount
        );
        return Err(SwapError::ExceededSlippage.into());
    }
    if token_b_amount == 0 {
        return Err(SwapError::ZeroTradingTokens.into());
    }

    let pool_token_amount = to_u64(pool_token_amount)?;

    msg!(
        "Deposit outputs: token_a_to_deposit={}, token_b_to_deposit={}, pool_tokens_to_mint={}",
        token_a_amount,
        token_b_amount,
        pool_token_amount,
    );

    swap_token::transfer_from_user(
        ctx.accounts.token_a_token_program.to_account_info(),
        ctx.accounts.token_a_user_ata.to_account_info(),
        ctx.accounts.token_a_mint.to_account_info(),
        ctx.accounts.token_a_vault.to_account_info(),
        ctx.accounts.signer.to_account_info(),
        token_a_amount,
        ctx.accounts.token_a_mint.decimals,
    )?;
    swap_token::transfer_from_user(
        ctx.accounts.token_b_token_program.to_account_info(),
        ctx.accounts.token_b_user_ata.to_account_info(),
        ctx.accounts.token_b_mint.to_account_info(),
        ctx.accounts.token_b_vault.to_account_info(),
        ctx.accounts.signer.to_account_info(),
        token_b_amount,
        ctx.accounts.token_b_mint.decimals,
    )?;

    pool_token::mint(
        ctx.accounts.pool_token_program.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.pool_token_mint.to_account_info(),
        ctx.accounts.pool_authority.to_account_info(),
        pool.pool_authority_bump_seed,
        ctx.accounts.pool_token_user_ata.to_account_info(),
        pool_token_amount,
    )?;

    Ok(SwapEvent::DepositAllTokenTypes {
        token_a_amount,
        token_b_amount,
        pool_token_amount,
    })
}

#[derive(Accounts)]
pub struct DepositAllTokenTypes<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut,
        has_one = swap_curve,
        has_one = pool_authority @ SwapError::InvalidProgramAddress,
        has_one = token_a_mint,
        has_one = token_b_mint,
        has_one = token_a_vault @ SwapError::IncorrectSwapAccount,
        has_one = token_b_vault @ SwapError::IncorrectSwapAccount,
        has_one = pool_token_mint @ SwapError::IncorrectPoolMint,
    )]
    pub pool: AccountLoader<'info, SwapPool>,

    /// CHECK: has_one constraint on the pool
    pub swap_curve: UncheckedAccount<'info>,

    /// CHECK: has_one constraint on the pool
    pub pool_authority: AccountInfo<'info>,

    /// CHECK: has_one constraint on the pool
    pub token_a_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// CHECK: has_one constraint on the pool
    pub token_b_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub token_a_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub token_b_vault: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_mint: Box<MultiProgramCompatibleAccount<'info, Mint>>,

    /// Signer's token A token account
    #[account(mut,
        token::mint = token_a_mint,
        token::authority = signer,
        token::token_program = token_a_token_program,
    )]
    pub token_a_user_ata: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

    /// Signer's token B token account
    #[account(mut,
        token::mint = token_b_mint,
        token::authority = signer,
        token::token_program = token_b_token_program,
    )]
    pub token_b_user_ata: Box<MultiProgramCompatibleAccount<'info, TokenAccount>>,

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
    pub token_a_token_program: CompatibleProgram<'info, Token>,
    /// Token program for the destination mint
    pub token_b_token_program: CompatibleProgram<'info, Token>,
}
