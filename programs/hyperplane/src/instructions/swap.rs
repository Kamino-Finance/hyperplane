use anchor_lang::{
    accounts::{interface::Interface, interface_account::InterfaceAccount},
    prelude::*,
};
use anchor_spl::{
    token_2022::spl_token_2022::extension::{
        transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions,
    },
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    curve,
    curve::{base::SwapCurve, calculator::TradeDirection},
    emitted,
    error::SwapError,
    event, require_msg,
    state::{SwapPool, SwapState},
    swap::utils::validate_inputs,
    to_u64, try_math,
    utils::{math::TryMath, swap_token},
};

pub fn handler(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<event::Swap> {
    let pool = ctx.accounts.pool.load()?;
    let trade_direction = validate_inputs(&ctx, &pool)?;
    let swap_curve = curve!(ctx.accounts.swap_curve, pool);

    // Take transfer fees into account for actual amount transferred in
    let actual_amount_in = utils::sub_input_transfer_fees(
        &ctx.accounts.source_mint.to_account_info(),
        &pool.fees,
        amount_in,
        ctx.accounts.source_token_host_fees_account.is_some(),
    )?;

    msg!(
        "Swap inputs: trade_direction={:?}, amount_in={}, actual_amount_in={}, minimum_amount_out={}",
        trade_direction,
        amount_in,
        actual_amount_in,
        minimum_amount_out
    );
    msg!(
        "Swap pool inputs: swap_type={:?}, source_token_balance={}, destination_token_balance={}",
        swap_curve.curve_type,
        ctx.accounts.source_vault.amount,
        ctx.accounts.destination_vault.amount,
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
    let source_amount_swapped =
        try_math!(to_u64!(result.source_amount_swapped)?.try_add(to_u64!(result.trade_fee)?))?;
    let source_transfer_amount = utils::add_transfer_fee(
        &ctx.accounts.source_mint.to_account_info(),
        source_amount_swapped,
    )?;

    let destination_amount_swapped = to_u64!(result.destination_amount_swapped)?;
    let destination_transfer_amount = utils::sub_transfer_fee(
        &ctx.accounts.destination_mint.to_account_info(),
        destination_amount_swapped,
    )?;

    msg!(
        "Swap result: source_amount_swapped={}, trade_fee={}, owner_fee={}, source_transfer_amount={}, destination_amount_swapped={}, destination_transfer_amount={}",
        result.source_amount_swapped,
        result.trade_fee,
        result.owner_fee,
        result.total_source_amount_swapped,
        destination_amount_swapped,
        destination_transfer_amount
    );
    require_msg!(
        destination_transfer_amount >= minimum_amount_out,
        SwapError::ExceededSlippage,
        &format!(
            "ExceededSlippage: amount_received={} < minimum_amount_out={}",
            destination_transfer_amount, minimum_amount_out
        )
    );

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
        let mut owner_fee = utils::add_transfer_fee(
            &ctx.accounts.source_mint.to_account_info(),
            to_u64!(result.owner_fee)?,
        )?
        .into();

        // Allow none to fall through
        if let Some(host_fees_account) = &ctx.accounts.source_token_host_fees_account {
            let host_fee = pool
                .fees()
                .host_fee(owner_fee)
                .map_err(|_| error!(SwapError::FeeCalculationFailure))?;
            if host_fee > 0 {
                owner_fee = try_math!(owner_fee.try_sub(host_fee))?;

                swap_token::transfer_from_user(
                    ctx.accounts.source_token_program.to_account_info(),
                    ctx.accounts.source_user_ata.to_account_info(),
                    ctx.accounts.source_mint.to_account_info(),
                    host_fees_account.to_account_info(),
                    ctx.accounts.signer.to_account_info(),
                    to_u64!(host_fee)?,
                    ctx.accounts.source_mint.decimals,
                )?;
            }
        }
        swap_token::transfer_from_user(
            ctx.accounts.source_token_program.to_account_info(),
            ctx.accounts.source_user_ata.to_account_info(),
            ctx.accounts.source_mint.to_account_info(),
            ctx.accounts.source_token_fees_vault.to_account_info(),
            ctx.accounts.signer.to_account_info(),
            to_u64!(owner_fee)?,
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
        destination_amount_swapped,
        ctx.accounts.destination_mint.decimals,
    )?;

    let fee = try_math!(result.owner_fee.try_add(result.trade_fee))?;
    let fee = to_u64!(fee)?;

    msg!(
        "Swap outputs: token_in_amount={:?}, token_out_amount={}, fee={}",
        source_transfer_amount,
        destination_amount_swapped,
        fee
    );
    emitted!(event::Swap {
        token_in_amount: source_transfer_amount,
        token_out_amount: destination_amount_swapped,
        fee,
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
    use crate::curve::fees::Fees;

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

    /// Subtract token mint transfer fees for actual amount transferred
    ///
    /// There are potentially 3 input transfers:
    /// 1. User -> Pool
    /// 2. User -> Fees
    /// 3. User -> Host Fees (optional)
    ///
    /// At low token amounts, the fees on each transfer rounding up can result in the user paying more than the amount_in, causing an unexpected `ExceededSlippage` error
    pub fn sub_input_transfer_fees(
        mint_acc_info: &AccountInfo,
        fees: &Fees,
        amount_in: u64,
        host_fee: bool,
    ) -> Result<u64> {
        let mint_data = mint_acc_info.data.borrow();
        let mint =
            StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
                &mint_data,
            )?;
        let amount = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
            let owner_fee = fees.owner_trading_fee(amount_in.into())?;
            let vault_amount_in = amount_in.saturating_sub(owner_fee as u64);

            let epoch = Clock::get()?.epoch;
            let vault_transfer_fee = transfer_fee_config
                .calculate_epoch_fee(epoch, vault_amount_in)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
            let (host_fee, host_transfer_fee) = if host_fee {
                let host_fee = fees.host_fee(owner_fee)?;
                (
                    host_fee,
                    transfer_fee_config
                        .calculate_epoch_fee(epoch, host_fee as u64)
                        .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?,
                )
            } else {
                (0, 0)
            };
            let owner_fee = owner_fee - host_fee;
            let owner_transfer_fee = transfer_fee_config
                .calculate_epoch_fee(epoch, owner_fee as u64)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;

            let amount_sub_fees = amount_in
                .saturating_sub(vault_transfer_fee)
                .saturating_sub(owner_transfer_fee)
                .saturating_sub(host_transfer_fee);

            msg!(
                "Subtract input token transfer fee: vault_transfer_amount={}, vault_transfer_fee={}, owner_fee={}, owner_fee_transfer_fee={}, host_fee={}, host_fee_transfer_fee={} amount={}, input_amount_sub_transfer_fees={}",
                vault_amount_in,
                vault_transfer_fee,
                owner_fee,
                owner_transfer_fee,
                host_fee,
                host_transfer_fee,
                amount_in,
                amount_sub_fees
            );
            amount_sub_fees
        } else {
            amount_in
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
