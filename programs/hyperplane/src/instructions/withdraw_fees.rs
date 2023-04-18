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
    utils::swap_token,
    withdraw_fees::utils::validate_inputs,
};

pub fn handler(
    ctx: Context<WithdrawFees>,
    requested_withdraw_amount: u64,
) -> Result<event::WithdrawFees> {
    let pool = ctx.accounts.pool.load()?;
    validate_inputs(&ctx, &pool)?;

    require_msg!(
        requested_withdraw_amount > 0,
        SwapError::ZeroTradingTokens,
        "Cannot withdraw zero pool tokens"
    );

    let withdraw_amount = cmp::min(requested_withdraw_amount, ctx.accounts.fees_vault.amount);

    msg!(
        "Withdrawing from fees vault: withdraw_amount={}, requested_withdraw_amount={}",
        withdraw_amount,
        requested_withdraw_amount,
    );

    swap_token::transfer_from_vault(
        ctx.accounts.fees_token_program.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.fees_vault.to_account_info(),
        ctx.accounts.fees_mint.to_account_info(),
        ctx.accounts.admin_fees_ata.to_account_info(),
        ctx.accounts.pool_authority.to_account_info(),
        pool.bump_seed(),
        withdraw_amount,
        ctx.accounts.fees_mint.decimals,
    )?;

    emitted!(event::WithdrawFees { withdraw_amount });
}

#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut,
        has_one = admin,
        has_one = pool_authority @ SwapError::InvalidProgramAddress,
    )]
    pub pool: AccountLoader<'info, SwapPool>,

    /// CHECK: has_one constraint on the pool
    pub pool_authority: AccountInfo<'info>,

    /// CHECK: checked in the handler
    #[account(
        token::token_program = fees_token_program,
    )]
    pub fees_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Fee vault to withdraw from
    /// CHECK: checked in the handler
    #[account(mut,
        constraint = fees_vault.amount > 0 @ SwapError::ZeroTradingTokens,
        token::token_program = fees_token_program,
    )]
    pub fees_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Admin's token account to withdraw fees to
    #[account(mut,
        token::mint = fees_mint,
        token::authority = admin,
        token::token_program = fees_token_program,
    )]
    pub admin_fees_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token program for the fee token mint
    pub fees_token_program: Interface<'info, TokenInterface>,
}

mod utils {
    use std::cell::Ref;

    use super::*;

    pub fn validate_inputs(ctx: &Context<WithdrawFees>, pool: &Ref<SwapPool>) -> Result<()> {
        if ctx.accounts.fees_mint.key() == pool.token_a_mint {
            require_msg!(
                pool.token_a_fees_vault == ctx.accounts.fees_vault.key(),
                SwapError::IncorrectFeeAccount,
                &format!(
                    "IncorrectFeeAccount: token_a_fees_vault.key ({}) != fees_vault.key ({})",
                    pool.token_a_fees_vault.key(),
                    ctx.accounts.fees_vault.key(),
                )
            );
        } else if ctx.accounts.fees_mint.key() == pool.token_b_mint {
            require_msg!(
                pool.token_b_fees_vault == ctx.accounts.fees_vault.key(),
                SwapError::IncorrectFeeAccount,
                &format!(
                    "IncorrectFeeAccount: token_b_fees_vault.key ({}) != fees_vault.key ({})",
                    pool.token_b_fees_vault.key(),
                    ctx.accounts.fees_vault.key(),
                )
            );
        } else {
            return err!(SwapError::IncorrectTradingMint);
        };

        Ok(())
    }
}
