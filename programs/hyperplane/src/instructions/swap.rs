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
    let source_amount_to_vault = to_u64!(result.source_amount_to_vault)?;
    let source_amount_to_vault = utils::add_inverse_transfer_fee(
        &ctx.accounts.source_mint.to_account_info(),
        source_amount_to_vault,
    )?;

    let destination_amount_from_vault = to_u64!(result.destination_amount_swapped)?;
    let destination_amount_post_transfer_fees = utils::sub_transfer_fee(
        &ctx.accounts.destination_mint.to_account_info(),
        destination_amount_from_vault,
    )?;

    msg!(
        "Swap result: total_source_debit_amount={}, source_amount_swapped={}, trade_fee={}, owner_fee={}, source_amount_to_vault={}, destination_amount_from_vault={}, destination_amount_post_transfer_fees={}",
        result.total_source_amount_swapped,
        result.source_amount_swapped,
        source_amount_to_vault,
        result.trade_fee,
        result.owner_fee,
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

    if result.owner_fee > 0 {
        let mut owner_fee = result.owner_fee;
        // Allow none to fall through
        if let Some(host_fees_account) = &ctx.accounts.source_token_host_fees_account {
            let host_fee = pool
                .fees()
                .host_fee(owner_fee)
                .map_err(|_| error!(SwapError::FeeCalculationFailure))?;
            if host_fee > 0 {
                owner_fee = try_math!(owner_fee.try_sub(host_fee))?;
                let host_fee = utils::add_inverse_transfer_fee(
                    &ctx.accounts.source_mint.to_account_info(),
                    to_u64!(host_fee)?,
                )?;

                swap_token::transfer_from_user(
                    ctx.accounts.source_token_program.to_account_info(),
                    ctx.accounts.source_user_ata.to_account_info(),
                    ctx.accounts.source_mint.to_account_info(),
                    host_fees_account.to_account_info(),
                    ctx.accounts.signer.to_account_info(),
                    host_fee,
                    ctx.accounts.source_mint.decimals,
                )?;
            }
        }
        let owner_fee = utils::add_inverse_transfer_fee(
            &ctx.accounts.source_mint.to_account_info(),
            to_u64!(owner_fee)?,
        )?;
        swap_token::transfer_from_user(
            ctx.accounts.source_token_program.to_account_info(),
            ctx.accounts.source_user_ata.to_account_info(),
            ctx.accounts.source_mint.to_account_info(),
            ctx.accounts.source_token_fees_vault.to_account_info(),
            ctx.accounts.signer.to_account_info(),
            owner_fee,
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

    let total_fees = to_u64!(result.total_fees)?;

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

    /// Subtract token mint transfer fees for actual amount received by the user post-transfer fees
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
            let amount_sub_fee = try_math!(amount.try_sub(transfer_fee))?;
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

    /// Subtract token mint transfer fees for actual amount received by the pool post-transfer fees
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
            let owner_and_host_fee = fees.owner_trading_fee(amount_in.into())?;
            let epoch = Clock::get()?.epoch;
            let (host_fee, host_transfer_fee) = if host_fee {
                let host_fee = fees.host_fee(owner_and_host_fee)?;
                (
                    host_fee,
                    transfer_fee_config
                        .calculate_epoch_fee(epoch, to_u64!(host_fee)?)
                        .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?,
                )
            } else {
                (0, 0)
            };
            let owner_fee = try_math!(owner_and_host_fee.try_sub(host_fee))?;
            let owner_transfer_fee = transfer_fee_config
                .calculate_epoch_fee(epoch, to_u64!(owner_fee)?)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;

            let vault_amount_in = try_math!(amount_in.try_sub(to_u64!(owner_and_host_fee)?))?;
            let vault_transfer_fee = transfer_fee_config
                .calculate_epoch_fee(epoch, vault_amount_in)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;

            let amount_sub_fees = try_math!(try_math!(try_math!(
                amount_in.try_sub(vault_transfer_fee)
            )?
            .try_sub(owner_transfer_fee))?
            .try_sub(host_transfer_fee))?;

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

    /// Add token mint transfer fees for actual amount sent pre-transfer fees
    pub fn add_inverse_transfer_fee(
        mint_acc_info: &AccountInfo,
        post_fee_amount: u64,
    ) -> Result<u64> {
        let mint_data = mint_acc_info.data.borrow();
        let mint =
            StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
                &mint_data,
            )?;
        let amount = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
            let transfer_fee = transfer_fee_config
                .calculate_inverse_epoch_fee(Clock::get()?.epoch, post_fee_amount)
                .ok_or_else(|| error!(SwapError::FeeCalculationFailure))?;
            let amount_add_fee = try_math!(post_fee_amount.try_add(transfer_fee))?;
            msg!(
                "Add token transfer fee: fee={}, amount={}, amount_add_fee={}",
                transfer_fee,
                post_fee_amount,
                amount_add_fee
            );
            amount_add_fee
        } else {
            post_fee_amount
        };
        Ok(amount)
    }

    #[cfg(test)]
    mod test {
        use anchor_lang::solana_program::{clock::Epoch, program_option::COption, pubkey::Pubkey};
        use anchor_spl::token_2022::{
            spl_token_2022,
            spl_token_2022::extension::{
                transfer_fee::TransferFee, ExtensionType, StateWithExtensionsMut,
            },
        };
        use proptest::{prop_assume, proptest};
        use spl_pod::optional_keys::OptionalNonZeroPubkey;

        use super::*;
        use crate::instructions::test::runner::syscall_stubs::test_syscall_stubs;

        #[test]
        pub fn test_sub_transfer_fee_when_no_transfer_fees() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 0);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount = sub_transfer_fee(&mint_info, 10_000).unwrap();

            assert_eq!(amount, 10_000);
        }

        #[test]
        pub fn test_sub_transfer_fee_when_10_bps_transfer_fee() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount = sub_transfer_fee(&mint_info, 10_000).unwrap();

            assert_eq!(amount, 9990);
        }

        #[test]
        pub fn test_sub_transfer_fee_rounds_up_when_small_fee() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount = sub_transfer_fee(&mint_info, 100).unwrap();

            assert_eq!(amount, 99);
        }

        #[test]
        pub fn test_add_inverse_transfer_fee_when_no_transfer_fees() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 0);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount = add_inverse_transfer_fee(&mint_info, 10_000).unwrap();

            assert_eq!(amount, 10_000);
        }

        #[test]
        pub fn test_add_inverse_transfer_fee_when_10_bps_transfer_fee() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount = add_inverse_transfer_fee(&mint_info, 9990).unwrap();

            assert_eq!(amount, 10_000);
        }

        #[test]
        pub fn test_add_inverse_transfer_fee_rounds_up_when_small_fee() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount = add_inverse_transfer_fee(&mint_info, 100).unwrap();

            assert_eq!(amount, 101);
        }

        #[test]
        pub fn test_sub_then_add_inverse_transfer_fee_when_10_bps_transfer_fee() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let receive_amount = sub_transfer_fee(&mint_info, 10_000_000).unwrap();
            let original = add_inverse_transfer_fee(&mint_info, receive_amount).unwrap();

            assert_eq!(original, 10_000_000);
        }

        #[test]
        pub fn test_sub_input_transfer_fee_when_no_transfer_fees_or_protocol_fees() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 0);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount =
                sub_input_transfer_fees(&mint_info, &Fees::default(), 10_000, false).unwrap();

            assert_eq!(amount, 10_000);
        }

        #[test]
        pub fn test_sub_input_transfer_fee_when_10bps_transfer_fees_and_no_protocol_fees() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let amount =
                sub_input_transfer_fees(&mint_info, &Fees::default(), 10_000, false).unwrap();

            // 1 transfer fee of 10 bps
            assert_eq!(amount, 9990);
        }

        #[test]
        pub fn test_sub_input_transfer_fee_when_10bps_transfer_fees_and_owner_protocol_fees() {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let fees = Fees {
                owner_trade_fee_numerator: 10,
                owner_trade_fee_denominator: 10_000,
                ..Default::default()
            };

            let amount = sub_input_transfer_fees(&mint_info, &fees, 10_000_000, false).unwrap();

            // Raw owner fee amount is 10_000 (10 bps of 10M)
            // Raw owner transfer fee is 10 (10 bps of 10_000)
            // Vault transfer amount is 9_990_000 (10M - 10_000)
            // not -10 because we re-take the owner fee from the total amount - all transfer fees
            // so the proportion of the owner fee is the same
            // vault transfer fee is 9990 (10 bps of 9_990_000)
            // 2 transfer fees equal to 10_000 total (9900 + 10)
            assert_eq!(amount, 9_990_000);
        }

        #[test]
        pub fn test_sub_input_transfer_fee_when_10bps_transfer_fees_and_owner_and_host_protocol_fees(
        ) {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let fees = Fees {
                owner_trade_fee_numerator: 10,
                owner_trade_fee_denominator: 10_000,
                host_fee_numerator: 10,
                host_fee_denominator: 10_000,
                ..Default::default()
            };

            let amount =
                sub_input_transfer_fees(&mint_info, &fees, 100_000_000_000_000, true).unwrap();

            // Owner fee amount is 100_000_000_000 (10 bps of 100_000B)
            // Host fee 10_000_000 (10 bps of 100_000_000_000) taken from the owner fee which is now 99_990_000_000 (100_000_000_000 - 10_000_000)
            // Owner transfer fee is 99_990_000 (10 bps of 99_990_000_000)
            // Host transfer fee is 10_000 (10 bps of 10_000_000)
            // Vault transfer amount is 99_900_000_000_000 (100_000B - 100_000_000_000)
            // vault transfer fee is 99_900_000_000 (10 bps of 99_900_000_000_000)
            // 3 transfer fees equal to 100_000_000_000 total (99_900_000_000 + 99_990_000 + 10_000)
            assert_eq!(amount, 99_900_000_000_000);
        }

        #[test]
        pub fn test_sub_input_transfer_fee_when_10bps_transfer_fees_and_owner_and_small_host_protocol_fees(
        ) {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let fees = Fees {
                owner_trade_fee_numerator: 10,
                owner_trade_fee_denominator: 10_000,
                host_fee_numerator: 10,
                host_fee_denominator: 10_000,
                ..Default::default()
            };

            let amount = sub_input_transfer_fees(&mint_info, &fees, 100_000_000, true).unwrap();

            // Owner fee amount is 100_000 (10 bps of 100M)
            // Host fee 100 (10 bps of 100_000) taken from the owner fee which is now 99_900 (100_000 - 100)
            // Owner transfer fee is 100 (10 bps of 100_000)
            // Host transfer fee is 1 (10 bps of 100 rounded up)
            // Vault transfer amount is 99_900_000 (100M - 100_000)
            // Vault transfer fee is 99_900 (10 bps of 99_900_000)
            // 3 transfer fees equal to 100_001 total (99_900 + 100 + 1)
            assert_eq!(amount, 99_899_999);
        }

        #[test]
        pub fn test_sub_input_transfer_fee_when_10bps_transfer_fees_and_both_owner_and_host_protocol_fees_small(
        ) {
            test_syscall_stubs();

            let mut mint_data = mint_with_fee_data();
            mint_with_transfer_fee(&mut mint_data, 10);

            let key = Pubkey::new_unique();
            let mut lamports = u64::MAX;
            let token_program = spl_token_2022::id();
            let mint_info = AccountInfo::new(
                &key,
                false,
                false,
                &mut lamports,
                &mut mint_data,
                &token_program,
                false,
                Epoch::default(),
            );

            let fees = Fees {
                owner_trade_fee_numerator: 10,
                owner_trade_fee_denominator: 10_000,
                host_fee_numerator: 10,
                host_fee_denominator: 10_000,
                ..Default::default()
            };

            let amount = sub_input_transfer_fees(&mint_info, &fees, 10_000_000, true).unwrap();

            // Owner fee amount is 10_000 (10 bps of 10M)
            // Host fee 10 (10 bps of 10_000) taken from the owner fee which is now 9_990 (10_000 - 10)
            // Owner transfer fee is 10 (10 bps of 9_990 rounded up)
            // Host transfer fee is 1 (10 bps of 9 rounded up)
            // Vault transfer amount is 9_990_000 (10M - 10_000)
            // Vault transfer fee is 9990 (10 bps of 9_990_000)
            // 3 transfer fees equal to 10_001 total (9990 + 10 + 1)
            assert_eq!(amount, 9_989_999);
        }

        proptest! {
            #[test]
            fn test_sub_then_add_inverse_transfer_fee_should_be_same_or_one_less(
                amount in 1..u32::MAX as u64,
                transfer_fee_bps in 0..10_000_u64,
            ) {
                test_syscall_stubs();

                let mut mint_data = mint_with_fee_data();
                mint_with_transfer_fee(&mut mint_data, 10);

                let key = Pubkey::new_unique();
                let mut lamports = u64::MAX;
                let token_program = spl_token_2022::id();
                let mint_info = AccountInfo::new(
                    &key,
                    false,
                    false,
                    &mut lamports,
                    &mut mint_data,
                    &token_program,
                    false,
                    Epoch::default(),
                );

                let receive_amount = sub_transfer_fee(&mint_info, amount).unwrap();
                let original = add_inverse_transfer_fee(&mint_info, receive_amount).unwrap();

                assert!(amount - original <= 1, "original: {}, amount: {}, diff: {}, transfer_fee_bps: {}, receive_amount={}", original, amount, amount - original, transfer_fee_bps, receive_amount);
            }
        }

        proptest! {
            #[test]
            fn test_sub_input_fees_same_or_less_after_re_adding(
                amount in 1..u32::MAX as u64,
                owner_trade_fee_numerator in 0..100_000_u64,
                owner_trade_fee_denominator in 1..100_000_u64,
                host_fee_numerator in 0..100_000_u64,
                host_fee_denominator in 1..100_000_u64,
                _transfer_fee_bps in 0..1000_u64,
                host_fees: bool,
            ) {
                // todo - fix bug where the user can be charged more than the amount in
                let transfer_fee_bps = 0;
                prop_assume!(host_fee_numerator <= host_fee_denominator);
                prop_assume!(owner_trade_fee_numerator <= owner_trade_fee_denominator);
                test_syscall_stubs();

                let mut mint_data = mint_with_fee_data();
                mint_with_transfer_fee(&mut mint_data, u16::try_from(transfer_fee_bps).unwrap());

                let key = Pubkey::new_unique();
                let mut lamports = u64::MAX;
                let token_program = spl_token_2022::id();
                let mint_info = AccountInfo::new(
                    &key,
                    false,
                    false,
                    &mut lamports,
                    &mut mint_data,
                    &token_program,
                    false,
                    Epoch::default(),
                );

                let fees = Fees {
                    owner_trade_fee_numerator,
                    owner_trade_fee_denominator,
                    host_fee_numerator,
                    host_fee_denominator,
                    ..Default::default()
                };

                let amount_sub_fees = sub_input_transfer_fees(&mint_info, &fees, amount, host_fees).unwrap();

                let estimated_transfer_fees = amount - amount_sub_fees;

                let owner_and_host_fee = fees.owner_trading_fee(amount_sub_fees.into()).unwrap();
                let host_fee = if host_fees {
                    fees.host_fee(owner_and_host_fee).unwrap() as u64
                } else {
                    0
                };

                let owner_fee = (owner_and_host_fee as u64).saturating_sub(host_fee);
                let vault_amount = amount_sub_fees.saturating_sub(owner_and_host_fee as u64);

                assert_eq!(amount_sub_fees, vault_amount + owner_fee + host_fee, "amount: {}, vault_amount: {}, host_and_owner_fee: {}, owner_fee: {}, host_fee: {}, amount_sub_fees: {}", amount, vault_amount, owner_and_host_fee, owner_fee, host_fee, amount_sub_fees);

                let vault_amount_add_fees = add_inverse_transfer_fee(&mint_info, vault_amount).unwrap();
                let owner_amount_add_fees = add_inverse_transfer_fee(&mint_info, owner_fee).unwrap();
                let host_amount_add_fees = if host_fees {
                    add_inverse_transfer_fee(&mint_info, host_fee).unwrap()
                } else {
                    0
                };

                let actual_vault_transfer_fee = vault_amount_add_fees - vault_amount;
                let actual_owner_transfer_fee = owner_amount_add_fees - owner_fee;
                let actual_host_transfer_fee = host_amount_add_fees - host_fee;
                let actual_transfer_fees = actual_vault_transfer_fee + actual_owner_transfer_fee + actual_host_transfer_fee;

                if host_fees {
                    let amount_with_fees = vault_amount_add_fees + owner_amount_add_fees + host_amount_add_fees;
                    let msg = format!("\namount={}\namount_with_xfer_fees={}\ntransfer_fee_bps={}\nestimated_transfer_fees={}\nactual_transfer_fees={}\nvault_amount={}\n\tvault_amount_xfer_fees={}\n\tvault_amount_add_xfer_fees={}\nowner_fee_amount={}\n\towner_xfer_fees={}\n\towner_amount_add_xfer_fees={}\nhost_fee_amount={}\n\thost_amount_xfer_fees={}\n\thost_amount_add_xfer_fees={}\nhost_and_owner_fee={}\namount_sub_xfer_fees={}\n", amount, amount_with_fees, transfer_fee_bps, estimated_transfer_fees, actual_transfer_fees, vault_amount, actual_vault_transfer_fee, vault_amount_add_fees, owner_fee, actual_owner_transfer_fee, owner_amount_add_fees, host_fee, actual_host_transfer_fee, host_amount_add_fees, owner_and_host_fee, amount_sub_fees);
                    assert!(amount_with_fees <= amount, "{}", msg);
                    let diff = amount - amount_with_fees;
                    assert!(diff <= 3, "\ndiff={}{}", diff, msg);
                } else {
                    let amount_with_fees = vault_amount_add_fees + owner_amount_add_fees;
                    let msg = format!("\namount={}\namount_with_xfer_fees={}\ntransfer_fee_bps={}\nestimated_transfer_fees={}\nactual_transfer_fees={}\nvault_amount={}\n\tvault_amount_xfer_fees={}\n\tvault_amount_add_xfer_fees={}\nowner_fee_amount={}\n\towner_xfer_fees={}\n\towner_amount_add_xfer_fees={}\namount_sub_xfer_fees={}\n", amount, amount_with_fees, transfer_fee_bps, estimated_transfer_fees, actual_transfer_fees, vault_amount, actual_vault_transfer_fee, vault_amount_add_fees, owner_fee, actual_owner_transfer_fee, owner_amount_add_fees, amount_sub_fees);
                    assert!(amount_with_fees <= amount, "{}", msg);
                    let diff = amount - amount_with_fees;
                    assert!(diff <= 2, "\ndiff={}{}", diff, msg);
                }
            }
        }

        proptest! {
            #[test]
            fn test_sub_input_fees_always_favours_pool_by_at_most_two_or_three(
                amount in 1..u32::MAX as u64,
                owner_trade_fee_numerator in 0..100_000_u64,
                owner_trade_fee_denominator in 1..100_000_u64,
                host_fee_numerator in 0..100_000_u64,
                host_fee_denominator in 1..100_000_u64,
                _transfer_fee_bps in 0..10_000_u64,
                host_fees: bool,
            ) {
                // todo - fix bug where the user can be charged more than the amount in
                let transfer_fee_bps = 0;
                prop_assume!(host_fee_numerator <= host_fee_denominator);
                prop_assume!(owner_trade_fee_numerator <= owner_trade_fee_denominator);
                test_syscall_stubs();

                let mut mint_data = mint_with_fee_data();
                mint_with_transfer_fee(&mut mint_data, u16::try_from(transfer_fee_bps).unwrap());

                let key = Pubkey::new_unique();
                let mut lamports = u64::MAX;
                let token_program = spl_token_2022::id();
                let mint_info = AccountInfo::new(
                    &key,
                    false,
                    false,
                    &mut lamports,
                    &mut mint_data,
                    &token_program,
                    false,
                    Epoch::default(),
                );

                let fees = Fees {
                    owner_trade_fee_numerator,
                    owner_trade_fee_denominator,
                    host_fee_numerator,
                    host_fee_denominator,
                    ..Default::default()
                };

                let amount_sub_fees = sub_input_transfer_fees(&mint_info, &fees, amount, host_fees).unwrap();
                // Compare with subtracting all fees at once
                let full_amount_sub_fees = sub_transfer_fee(&mint_info, amount).unwrap();

                if host_fees {
                    // At most a difference of 3 due to rounding from 3 transfers - 1 to the pool, 1 to the owner fees vault, 1 to the host account
                    assert!(full_amount_sub_fees <= amount_sub_fees && amount_sub_fees - full_amount_sub_fees <= 3, "\nfull_amount_sub_fees should be greater than amount_sub_fees by at most 3.\namount={}\namount_sub_fees={}\nfull_amount_sub_fees={}\n", amount, amount_sub_fees, full_amount_sub_fees);
                } else {
                    assert!(full_amount_sub_fees <= amount_sub_fees && amount_sub_fees - full_amount_sub_fees <= 2, "\nfull_amount_sub_fees should be greater than amount_sub_fees by at most 2.\namount={}\namount_sub_fees={}\nfull_amount_sub_fees={}\n", amount, amount_sub_fees, full_amount_sub_fees);
                }
            }
        }

        fn mint_with_transfer_fee(mint_data: &mut [u8], transfer_fee_bps: u16) {
            let mut mint =
                StateWithExtensionsMut::<spl_token_2022::state::Mint>::unpack_uninitialized(
                    mint_data,
                )
                .unwrap();
            let extension = mint.init_extension::<TransferFeeConfig>(true).unwrap();
            extension.transfer_fee_config_authority = OptionalNonZeroPubkey::default();
            extension.withdraw_withheld_authority = OptionalNonZeroPubkey::default();
            extension.withheld_amount = 0u64.into();

            let epoch = Clock::get().unwrap().epoch;
            let transfer_fee = TransferFee {
                epoch: epoch.into(),
                transfer_fee_basis_points: transfer_fee_bps.into(),
                maximum_fee: u64::MAX.into(),
            };
            extension.older_transfer_fee = transfer_fee;
            extension.newer_transfer_fee = transfer_fee;

            mint.base.decimals = 6;
            mint.base.is_initialized = true;
            mint.base.mint_authority = COption::Some(Pubkey::new_unique());
            mint.pack_base();
            mint.init_account_type().unwrap();
        }

        fn mint_with_fee_data() -> Vec<u8> {
            vec![
                0;
                ExtensionType::get_account_len::<spl_token_2022::state::Mint>(&[
                    ExtensionType::TransferFeeConfig
                ])
            ]
        }
    }
}
