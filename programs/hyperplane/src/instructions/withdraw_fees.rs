use std::cmp;

use anchor_lang::{
    accounts::{interface::Interface, interface_account::InterfaceAccount},
    prelude::*,
};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{
    emitted,
    error::SwapError,
    event, require_msg,
    state::{SwapPool, SwapState},
    utils::pool_token,
};

pub fn handler(
    ctx: Context<WithdrawFees>,
    requested_pool_token_amount: u64,
) -> Result<event::WithdrawFees> {
    let pool = ctx.accounts.pool.load()?;

    require_msg!(
        requested_pool_token_amount > 0,
        SwapError::ZeroTradingTokens,
        "Cannot withdraw zero pool tokens"
    );
    require_msg!(
        ctx.accounts.pool_token_fees_vault.amount > 0,
        SwapError::ZeroTradingTokens,
        "Fee vault is empty"
    );

    let pool_token_amount = cmp::min(
        requested_pool_token_amount,
        ctx.accounts.pool_token_fees_vault.amount,
    );

    msg!(
        "Withdrawing from fees vault: pool_token_amount={}, requested_pool_token_amount={}",
        pool_token_amount,
        requested_pool_token_amount,
    );

    pool_token::transfer_from_vault(
        ctx.accounts.pool_token_program.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.pool_token_fees_vault.to_account_info(),
        ctx.accounts.pool_token_mint.to_account_info(),
        ctx.accounts.admin_pool_token_ata.to_account_info(),
        ctx.accounts.pool_authority.to_account_info(),
        pool.bump_seed(),
        pool_token_amount,
        ctx.accounts.pool_token_mint.decimals,
    )?;

    emitted!(event::WithdrawFees { pool_token_amount });
}

#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut,
        has_one = admin,
        has_one = pool_authority @ SwapError::InvalidProgramAddress,
        has_one = pool_token_mint @ SwapError::IncorrectPoolMint,
        has_one = pool_token_fees_vault @ SwapError::IncorrectFeeAccount,
    )]
    pub pool: AccountLoader<'info, SwapPool>,

    /// CHECK: has_one constraint on the pool
    pub pool_authority: AccountInfo<'info>,

    /// CHECK: has_one constraint on the pool
    #[account(
        token::token_program = pool_token_program,
    )]
    pub pool_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Account to withdraw fees from
    /// CHECK: has_one constraint on the pool
    #[account(mut,
        token::token_program = pool_token_program,
    )]
    pub pool_token_fees_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Admin's pool token account to withdraw fees to
    #[account(mut,
        token::mint = pool_token_mint,
        token::authority = admin,
        token::token_program = pool_token_program,
    )]
    pub admin_pool_token_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token program for the pool token mint
    pub pool_token_program: Interface<'info, TokenInterface>,
}
