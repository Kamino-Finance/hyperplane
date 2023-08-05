use anchor_lang::{
    accounts::{interface::Interface, interface_account::InterfaceAccount},
    prelude::*,
};
use anchor_spl::{
    token_2022::spl_token_2022::extension::StateWithExtensions,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    curve,
    curve::{
        base::{SwapCurve, SwapFeeInputs},
        calculator::TradeDirection,
    },
    emitted,
    error::SwapError,
    event, require_msg,
    state::{SwapPool, SwapState},
    swap::utils::validate_inputs,
    to_u64,
    utils::{swap_token, token_2022, token_2022::get_transfer_fee_config},
};

pub fn handler(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<event::Swap> {
    let pool = ctx.accounts.pool.load()?;
    let trade_direction = validate_inputs(&ctx, &pool)?;
    let swap_curve = curve!(ctx.accounts.swap_curve, pool);

    msg!(
        "Swap inputs: trade_direction={:?}, amount_in={}, minimum_amount_out={}",
        trade_direction,
        amount_in,
        minimum_amount_out
    );
    msg!(
        "Swap pool inputs: swap_type={:?}, source_token_balance={}, destination_token_balance={}",
        swap_curve.curve_type,
        ctx.accounts.source_vault.amount,
        ctx.accounts.destination_vault.amount,
    );
    let source_mint_info = ctx.accounts.source_mint.to_account_info();
    let mint_data = source_mint_info.data.borrow();
    let source_mint =
        StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
            &mint_data,
        )?;
    let transfer_fees = get_transfer_fee_config(&source_mint);
    let result = swap_curve
        .swap(
            u128::from(amount_in),
            u128::from(ctx.accounts.source_vault.amount),
            u128::from(ctx.accounts.destination_vault.amount),
            trade_direction,
            &SwapFeeInputs {
                transfer_fees,
                pool_fees: pool.fees(),
                host_fees: ctx.accounts.source_token_host_fees_account.is_some(),
            },
        )
        .map_err(|_| error!(SwapError::ZeroTradingTokens))?;

    let source_amount_to_vault = to_u64!(result.source_amount_to_vault)?;

    let destination_amount_from_vault = to_u64!(result.destination_amount_swapped)?;
    let destination_amount_post_transfer_fees = token_2022::sub_transfer_fee2(
        &ctx.accounts.destination_mint.to_account_info(),
        destination_amount_from_vault,
    )?;

    msg!(
        "Swap result: source_amount_swapped={}, trade_fee={}, owner_fee={}, source_amount_to_vault={}, destination_amount_from_vault={}, destination_amount_post_transfer_fees={}",
        result.source_amount_swapped,
        result.trade_fee,
        result.owner_fee,
        source_amount_to_vault,
        destination_amount_from_vault,
        destination_amount_post_transfer_fees
    );
    require_msg!(
        destination_amount_post_transfer_fees >= minimum_amount_out,
        SwapError::ExceededSlippage,
        &format!(
            "ExceededSlippage: amount_received={} < minimum_amount_out={}",
            destination_amount_post_transfer_fees, minimum_amount_out
        )
    );

    swap_token::transfer_from_user(
        ctx.accounts.source_token_program.to_account_info(),
        ctx.accounts.source_user_ata.to_account_info(),
        ctx.accounts.source_mint.to_account_info(),
        ctx.accounts.source_vault.to_account_info(),
        ctx.accounts.signer.to_account_info(),
        source_amount_to_vault,
        ctx.accounts.source_mint.decimals,
    )?;

    if result.host_fee > 0 {
        if let Some(host_fees_acc) = &ctx.accounts.source_token_host_fees_account {
            swap_token::transfer_from_user(
                ctx.accounts.source_token_program.to_account_info(),
                ctx.accounts.source_user_ata.to_account_info(),
                ctx.accounts.source_mint.to_account_info(),
                host_fees_acc.to_account_info(),
                ctx.accounts.signer.to_account_info(),
                to_u64!(result.host_fee)?,
                ctx.accounts.source_mint.decimals,
            )?;
        }
    }

    if result.owner_fee > 0 {
        swap_token::transfer_from_user(
            ctx.accounts.source_token_program.to_account_info(),
            ctx.accounts.source_user_ata.to_account_info(),
            ctx.accounts.source_mint.to_account_info(),
            ctx.accounts.source_token_fees_vault.to_account_info(),
            ctx.accounts.signer.to_account_info(),
            to_u64!(result.owner_fee)?,
            ctx.accounts.source_mint.decimals,
        )?;
    }

    swap_token::transfer_from_vault(
        ctx.accounts.destination_token_program.to_account_info(),
        ctx.accounts.pool.to_account_info(),
        ctx.accounts.destination_vault.to_account_info(),
        ctx.accounts.destination_mint.to_account_info(),
        ctx.accounts.destination_user_ata.to_account_info(),
        ctx.accounts.pool_authority.to_account_info(),
        pool.bump_seed(),
        destination_amount_from_vault,
        ctx.accounts.destination_mint.decimals,
    )?;

    let total_fees = result.total_fees()?;
    let total_fees = to_u64!(total_fees)?;

    msg!(
        "Swap outputs: token_in_amount={}, token_out_amount={}, total_fees={}",
        source_amount_to_vault,
        destination_amount_from_vault,
        total_fees
    );
    emitted!(event::Swap {
        token_in_amount: source_amount_to_vault,
        token_out_amount: destination_amount_from_vault,
        total_fees,
    });
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut,
    has_one = swap_curve,
    has_one = pool_authority @ SwapError::InvalidProgramAddress,
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

    /// Account to collect fees into
    /// CHECK: has_one constraint on the pool
    #[account(mut)]
    pub source_token_fees_vault: Box<InterfaceAccount<'info, TokenAccount>>,

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

    /// Optional pool token fees account for front ends - if not present, all fees are sent to the trading fees account
    #[account(mut,
    token::mint = source_mint,
    token::token_program = source_token_program,
    )]
    pub source_token_host_fees_account: Option<Box<InterfaceAccount<'info, TokenAccount>>>,

    /// Token program for the source mint
    pub source_token_program: Interface<'info, TokenInterface>,
    /// Token program for the destination mint
    pub destination_token_program: Interface<'info, TokenInterface>,
}

mod utils {
    use std::cell::Ref;

    use super::*;

    pub fn validate_inputs(ctx: &Context<Swap>, pool: &Ref<SwapPool>) -> Result<TradeDirection> {
        require_msg!(
            !pool.withdrawals_only(),
            SwapError::WithdrawalsOnlyMode,
            "The pool is in withdrawals only mode"
        );
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
                require_msg!(
                    ctx.accounts.source_token_fees_vault.key() == pool.token_a_fees_vault,
                    SwapError::IncorrectSwapAccount,
                    &format!(
                        "IncorrectSwapAccount: source_token_fees_vault.key ({}) != token_a_fees_vault.key ({})",
                        ctx.accounts.source_token_fees_vault.key(),
                        pool.token_a_fees_vault.key()
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
                require_msg!(
                    ctx.accounts.source_token_fees_vault.key() == pool.token_b_fees_vault,
                    SwapError::IncorrectSwapAccount,
                    &format!(
                        "IncorrectSwapAccount: source_token_fees_vault.key ({}) != token_b_fees_vault.key ({})",
                        ctx.accounts.source_token_fees_vault.key(),
                        pool.token_b_fees_vault.key()
                    )
                );
            }
        };

        Ok(trade_direction)
    }
}
