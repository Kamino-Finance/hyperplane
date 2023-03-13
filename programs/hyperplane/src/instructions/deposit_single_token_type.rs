use anchor_lang::{
    accounts::{interface::Interface, interface_account::InterfaceAccount},
    prelude::*,
};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{
    curve,
    curve::{base::SwapCurve, calculator::TradeDirection},
    deposit_single_token_type::utils::validate_swap_inputs,
    emitted,
    error::SwapError,
    event, require_msg,
    state::{SwapPool, SwapState},
    to_u64,
    utils::{pool_token, swap_token},
};

pub fn handler(
    ctx: Context<DepositSingleTokenType>,
    source_token_amount: u64,
    minimum_pool_token_amount: u64,
) -> Result<event::DepositSingleTokenType> {
    let pool = ctx.accounts.pool.load()?;
    let trade_direction = validate_swap_inputs(&ctx, &pool)?;
    msg!(
        "Deposit inputs: trade_direction={:?}, source_token_amount={}, minimum_pool_token_amount={}",
        trade_direction,
        source_token_amount,
        minimum_pool_token_amount,
    );
    let swap_curve = curve!(ctx.accounts.swap_curve, pool);

    let calculator = &swap_curve.calculator;
    require!(
        calculator.allows_deposits(),
        SwapError::UnsupportedCurveOperation
    );

    msg!(
        "Swap pool inputs: swap_type={:?}, token_a_balance={}, token_b_balance={}, pool_token_supply={}",
        swap_curve.curve_type,
        ctx.accounts.token_a_vault.amount,
        ctx.accounts.token_b_vault.amount,
        ctx.accounts.pool_token_mint.supply,
    );
    let pool_mint_supply = u128::from(ctx.accounts.pool_token_mint.supply);
    let pool_token_amount = if pool_mint_supply > 0 {
        swap_curve
            .deposit_single_token_type(
                u128::from(source_token_amount),
                u128::from(ctx.accounts.token_a_vault.amount),
                u128::from(ctx.accounts.token_b_vault.amount),
                pool_mint_supply,
                trade_direction,
                pool.fees(),
            )
            .map_err(|_| error!(SwapError::ZeroTradingTokens))?
    } else {
        calculator.new_pool_supply()
    };

    let pool_token_amount = to_u64!(pool_token_amount)?;

    require_msg!(
        pool_token_amount >= minimum_pool_token_amount,
        SwapError::ExceededSlippage,
        &format!(
            "ExceededSlippage: pool_token_amount={} < minimum_pool_token_amount={}",
            pool_token_amount, minimum_pool_token_amount
        )
    );
    require!(pool_token_amount > 0, SwapError::ZeroTradingTokens);

    msg!(
        "Deposit outputs: source_token_amount={}, pool_tokens_to_burn={}",
        source_token_amount,
        pool_token_amount,
    );

    let destination_vault = match trade_direction {
        TradeDirection::AtoB => &ctx.accounts.token_a_vault,
        TradeDirection::BtoA => &ctx.accounts.token_b_vault,
    };
    swap_token::transfer_from_user(
        ctx.accounts.source_token_program.to_account_info(),
        ctx.accounts.source_token_user_ata.to_account_info(),
        ctx.accounts.source_token_mint.to_account_info(),
        destination_vault.to_account_info(),
        ctx.accounts.signer.to_account_info(),
        source_token_amount,
        ctx.accounts.source_token_mint.decimals,
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

    emitted!(event::DepositSingleTokenType {
        token_amount: source_token_amount,
        pool_token_amount,
    });
}

#[derive(Accounts)]
pub struct DepositSingleTokenType<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut,
        has_one = swap_curve,
        has_one = pool_authority @ SwapError::InvalidProgramAddress,
        has_one = token_a_vault @ SwapError::IncorrectSwapAccount,
        has_one = token_b_vault @ SwapError::IncorrectSwapAccount,
        has_one = pool_token_mint @ SwapError::IncorrectPoolMint,
    )]
    pub pool: AccountLoader<'info, SwapPool>,

    /// CHECK: has_one constraint on the pool
    pub swap_curve: UncheckedAccount<'info>,

    /// CHECK: has_one constraint on the pool
    pub pool_authority: AccountInfo<'info>,

    /// CHECK: checked in the handler
    pub source_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub token_a_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub token_b_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub pool_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Signer's source token account
    // note - authority constraint repeated for clarity
    #[account(mut,
        token::mint = source_token_mint,
        token::authority = pool_token_user_ata.owner,
        token::token_program = source_token_program,
    )]
    pub source_token_user_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Signer's pool token account
    // note - authority constraint repeated for clarity
    #[account(mut,
        token::mint = pool_token_mint,
        token::authority = source_token_user_ata.owner,
        token::token_program = pool_token_program,
    )]
    pub pool_token_user_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token program for the pool token mint
    pub pool_token_program: Interface<'info, TokenInterface>,
    /// Token program for the source mint
    pub source_token_program: Interface<'info, TokenInterface>,
}

mod utils {
    use std::cell::Ref;

    use super::*;

    pub fn validate_swap_inputs(
        ctx: &Context<DepositSingleTokenType>,
        pool: &Ref<SwapPool>,
    ) -> Result<TradeDirection> {
        let trade_direction = if ctx.accounts.source_token_user_ata.mint
            == ctx.accounts.token_a_vault.mint
        {
            require_msg!(
                pool.token_a_vault != ctx.accounts.source_token_user_ata.key(),
                SwapError::IncorrectSwapAccount,
                &format!("IncorrectSwapAccount: source_token_user_ata.key ({}) == token_a_vault.key ({})", 
                    ctx.accounts.source_token_user_ata.key(), pool.token_a_vault.key()
                )
            );
            TradeDirection::AtoB
        } else if ctx.accounts.source_token_user_ata.mint == ctx.accounts.token_b_vault.mint {
            require_msg!(
                pool.token_b_vault != ctx.accounts.source_token_user_ata.key(),
                SwapError::IncorrectSwapAccount,
                &format!("IncorrectSwapAccount: source_token_user_ata.key ({}) == token_b_vault.key ({})", 
                    ctx.accounts.source_token_user_ata.key(), pool.token_a_vault.key()
                )
            );
            TradeDirection::BtoA
        } else {
            msg!("IncorrectSwapAccount: source_token_user_ata.mint ({}) != token_a_vault.mint ({}) || token_b_vault.mint ({})", ctx.accounts.source_token_user_ata.mint, ctx.accounts.token_a_vault.mint, ctx.accounts.token_b_vault.mint);
            return err!(SwapError::IncorrectSwapAccount);
        };

        Ok(trade_direction)
    }
}
