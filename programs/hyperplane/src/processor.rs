//! Program state processor

use std::error::Error;

use anchor_lang::prelude::AccountLoader;
use anchor_lang::solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    decode_error::DecodeError,
    entrypoint::ProgramResult,
    instruction::Instruction,
    msg,
    program::invoke_signed,
    program_error::{PrintProgramError, ProgramError},
    program_option::COption,
    pubkey::Pubkey,
    sysvar::Sysvar,
};
use num_traits::FromPrimitive;
use spl_token_2022::{
    check_spl_token_program_account,
    error::TokenError,
    extension::{
        mint_close_authority::MintCloseAuthority, transfer_fee::TransferFeeConfig,
        BaseStateWithExtensions, StateWithExtensions,
    },
    state::{Account, Mint},
};

use crate::constraints::{SwapConstraints, SWAP_CONSTRAINTS};
use crate::utils::math::{to_u128, to_u64};
use crate::{
    curve,
    curve::{
        base::SwapCurve,
        calculator::{RoundDirection, TradeDirection},
        fees::Fees,
    },
    error::SwapError,
    ix::SwapInstruction,
    state::{SwapPool, SwapState},
};

/// Program state handler.
pub struct Processor {}
impl Processor {
    /// Unpacks a spl_token `Account`.
    pub fn unpack_token_account(
        account_info: &AccountInfo,
        token_program_id: &Pubkey,
    ) -> Result<Account, SwapError> {
        // todo - elliot - this && will be removed
        if account_info.owner != token_program_id
            && check_spl_token_program_account(account_info.owner).is_err()
        {
            Err(SwapError::IncorrectTokenProgramId)
        } else {
            StateWithExtensions::<Account>::unpack(&account_info.data.borrow())
                .map(|a| a.base)
                .map_err(|_| SwapError::ExpectedAccount)
        }
    }

    /// Unpacks a spl_token `Mint`.
    pub fn unpack_mint(
        account_info: &AccountInfo,
        token_program_id: &Pubkey,
    ) -> Result<Mint, SwapError> {
        // todo - elliot - this && will be removed
        if account_info.owner != token_program_id
            && check_spl_token_program_account(account_info.owner).is_err()
        {
            Err(SwapError::IncorrectTokenProgramId)
        } else {
            StateWithExtensions::<Mint>::unpack(&account_info.data.borrow())
                .map(|m| m.base)
                .map_err(|_| SwapError::ExpectedMint)
        }
    }

    /// Unpacks a spl_token `Mint` with extension data
    pub fn unpack_mint_with_extensions<'a>(
        account_data: &'a [u8],
        owner: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<StateWithExtensions<'a, Mint>, SwapError> {
        if owner != token_program_id && check_spl_token_program_account(owner).is_err() {
            Err(SwapError::IncorrectTokenProgramId)
        } else {
            StateWithExtensions::<Mint>::unpack(account_data).map_err(|_| SwapError::ExpectedMint)
        }
    }

    /// Calculates the authority id by generating a program address.
    pub fn authority_id(
        program_id: &Pubkey,
        my_info: &Pubkey,
        bump_seed: u8,
    ) -> Result<Pubkey, SwapError> {
        Pubkey::create_program_address(
            &[
                b"pauthority".as_ref(),
                &my_info.to_bytes()[..32],
                &[bump_seed],
            ],
            program_id,
        )
        .or(Err(SwapError::InvalidProgramAddress))
    }

    /// Issue a spl_token `Burn` instruction.
    pub fn token_burn<'a>(
        swap: &Pubkey,
        token_program: AccountInfo<'a>,
        burn_account: AccountInfo<'a>,
        mint: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        bump_seed: u8,
        amount: u64,
    ) -> Result<(), ProgramError> {
        let swap_bytes = swap.to_bytes();
        let authority_signature_seeds = [b"pauthority".as_ref(), &swap_bytes[..32], &[bump_seed]];
        let signers = &[&authority_signature_seeds[..]];

        let ix = spl_token_2022::instruction::burn(
            token_program.key,
            burn_account.key,
            mint.key,
            authority.key,
            &[],
            amount,
        )?;

        invoke_signed_wrapper::<TokenError>(
            &ix,
            &[burn_account, mint, authority, token_program],
            signers,
        )
    }

    /// Issue a spl_token `MintTo` instruction.
    pub fn token_mint_to<'a>(
        swap: &Pubkey,
        token_program: AccountInfo<'a>,
        mint: AccountInfo<'a>,
        destination: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        bump_seed: u8,
        amount: u64,
    ) -> Result<(), ProgramError> {
        let swap_bytes = swap.to_bytes();
        let authority_signature_seeds = [b"pauthority".as_ref(), &swap_bytes[..32], &[bump_seed]];
        let signers = &[&authority_signature_seeds[..]];
        let ix = spl_token_2022::instruction::mint_to(
            token_program.key,
            mint.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?;

        invoke_signed_wrapper::<TokenError>(
            &ix,
            &[mint, destination, authority, token_program],
            signers,
        )
    }

    /// Issue a spl_token `Transfer` instruction.
    #[allow(clippy::too_many_arguments)]
    pub fn token_transfer<'a>(
        swap: &Pubkey,
        token_program: AccountInfo<'a>,
        source: AccountInfo<'a>,
        mint: AccountInfo<'a>,
        destination: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        bump_seed: u8,
        amount: u64,
        decimals: u8,
    ) -> Result<(), ProgramError> {
        let swap_bytes = swap.to_bytes();
        // todo - elliot - these seeds should not be used?
        let authority_signature_seeds = [b"pauthority".as_ref(), &swap_bytes[..32], &[bump_seed]];
        let signers = &[&authority_signature_seeds[..]];
        let ix = spl_token_2022::instruction::transfer_checked(
            token_program.key,
            source.key,
            mint.key,
            destination.key,
            authority.key,
            &[],
            amount,
            decimals,
        )?;
        invoke_signed_wrapper::<TokenError>(
            &ix,
            &[source, mint, destination, authority, token_program],
            signers,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn check_accounts(
        token_swap: &SwapPool,
        program_id: &Pubkey,
        swap_account_info: &AccountInfo,
        authority_info: &AccountInfo,
        token_a_info: &AccountInfo,
        token_b_info: &AccountInfo,
        pool_mint_info: &AccountInfo,
        pool_token_program_info: &AccountInfo,
        user_token_a_info: Option<&AccountInfo>,
        user_token_b_info: Option<&AccountInfo>,
        pool_fee_account_info: Option<&AccountInfo>,
    ) -> ProgramResult {
        if swap_account_info.owner != program_id {
            return Err(ProgramError::IncorrectProgramId);
        }
        if *authority_info.key
            != Self::authority_id(program_id, swap_account_info.key, token_swap.bump_seed())?
        {
            return Err(SwapError::InvalidProgramAddress.into());
        }
        if *token_a_info.key != *token_swap.token_a_account() {
            return Err(SwapError::IncorrectSwapAccount.into());
        }
        if *token_b_info.key != *token_swap.token_b_account() {
            return Err(SwapError::IncorrectSwapAccount.into());
        }
        if *pool_mint_info.key != *token_swap.pool_mint() {
            return Err(SwapError::IncorrectPoolMint.into());
        }
        if *pool_token_program_info.key != *token_swap.token_program_id() {
            return Err(SwapError::IncorrectTokenProgramId.into());
        }
        if let Some(user_token_a_info) = user_token_a_info {
            if token_a_info.key == user_token_a_info.key {
                return Err(SwapError::InvalidInput.into());
            }
        }
        if let Some(user_token_b_info) = user_token_b_info {
            if token_b_info.key == user_token_b_info.key {
                return Err(SwapError::InvalidInput.into());
            }
        }
        if let Some(pool_fee_account_info) = pool_fee_account_info {
            if *pool_fee_account_info.key != *token_swap.pool_fee_account() {
                return Err(SwapError::IncorrectFeeAccount.into());
            }
        }
        Ok(())
    }

    // todo - elliot - this method will be deleted but kept currently for reference
    /// Processes an [Initialize](enum.Instruction.html).
    pub fn process_initialize(
        program_id: &Pubkey,
        fees: Fees,
        swap_curve: SwapCurve,
        accounts: &[AccountInfo],
        swap_constraints: &Option<SwapConstraints>,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let swap_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let token_a_info = next_account_info(account_info_iter)?;
        let token_b_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let fee_account_info = next_account_info(account_info_iter)?;
        let destination_info = next_account_info(account_info_iter)?;
        let pool_token_program_info = next_account_info(account_info_iter)?;

        let token_program_id = *pool_token_program_info.key;
        // this method will be deleted
        // if SwapVersion::is_initialized(&swap_info.data.borrow()) {
        //     return Err(SwapError::AlreadyInUse.into());
        // }

        let (swap_authority, bump_seed) =
            Pubkey::find_program_address(&[&swap_info.key.to_bytes()], program_id);
        if *authority_info.key != swap_authority {
            return Err(SwapError::InvalidProgramAddress.into());
        }
        let token_a = Self::unpack_token_account(token_a_info, &token_program_id)?;
        let token_b = Self::unpack_token_account(token_b_info, &token_program_id)?;
        let fee_account = Self::unpack_token_account(fee_account_info, &token_program_id)?;
        let destination = Self::unpack_token_account(destination_info, &token_program_id)?;
        let pool_mint = {
            let pool_mint_data = pool_mint_info.data.borrow();
            let pool_mint = Self::unpack_mint_with_extensions(
                &pool_mint_data,
                pool_mint_info.owner,
                &token_program_id,
            )?;
            if let Ok(extension) = pool_mint.get_extension::<MintCloseAuthority>() {
                let close_authority: Option<Pubkey> = extension.close_authority.into();
                if close_authority.is_some() {
                    return Err(SwapError::InvalidCloseAuthority.into());
                }
            }
            pool_mint.base
        };
        if *authority_info.key != token_a.owner {
            return Err(SwapError::InvalidOwner.into());
        }
        if *authority_info.key != token_b.owner {
            return Err(SwapError::InvalidOwner.into());
        }
        if *authority_info.key == destination.owner {
            return Err(SwapError::InvalidOutputOwner.into());
        }
        if *authority_info.key == fee_account.owner {
            return Err(SwapError::InvalidOutputOwner.into());
        }
        if COption::Some(*authority_info.key) != pool_mint.mint_authority {
            return Err(SwapError::InvalidOwner.into());
        }

        if token_a.mint == token_b.mint {
            return Err(SwapError::RepeatedMint.into());
        }
        swap_curve
            .calculator
            .validate_supply(token_a.amount, token_b.amount)?;
        if token_a.delegate.is_some() {
            return Err(SwapError::InvalidDelegate.into());
        }
        if token_b.delegate.is_some() {
            return Err(SwapError::InvalidDelegate.into());
        }
        if token_a.close_authority.is_some() {
            return Err(SwapError::InvalidCloseAuthority.into());
        }
        if token_b.close_authority.is_some() {
            return Err(SwapError::InvalidCloseAuthority.into());
        }

        if pool_mint.supply != 0 {
            return Err(SwapError::InvalidSupply.into());
        }
        if pool_mint.freeze_authority.is_some() {
            return Err(SwapError::InvalidFreezeAuthority.into());
        }
        if *pool_mint_info.key != fee_account.mint {
            return Err(SwapError::IncorrectPoolMint.into());
        }

        if let Some(swap_constraints) = swap_constraints {
            let owner_key = swap_constraints
                .owner_key
                .parse::<Pubkey>()
                .map_err(|_| SwapError::InvalidOwner)?;
            if fee_account.owner != owner_key {
                return Err(SwapError::InvalidOwner.into());
            }
            swap_constraints.validate_curve(&swap_curve)?;
            swap_constraints.validate_fees(&fees)?;
        }
        fees.validate()?;
        swap_curve.calculator.validate()?;

        let initial_amount = swap_curve.calculator.new_pool_supply();

        Self::token_mint_to(
            swap_info.key,
            pool_token_program_info.clone(),
            pool_mint_info.clone(),
            destination_info.clone(),
            authority_info.clone(),
            bump_seed,
            to_u64(initial_amount)?,
        )?;

        Ok(())
    }

    /// Processes an [Swap](enum.Instruction.html).
    pub fn process_swap(
        program_id: &Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let swap_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let user_transfer_authority_info = next_account_info(account_info_iter)?;
        let source_info = next_account_info(account_info_iter)?;
        let swap_source_info = next_account_info(account_info_iter)?;
        let swap_destination_info = next_account_info(account_info_iter)?;
        let destination_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let pool_fee_account_info = next_account_info(account_info_iter)?;
        let source_token_mint_info = next_account_info(account_info_iter)?;
        let destination_token_mint_info = next_account_info(account_info_iter)?;
        let source_token_program_info = next_account_info(account_info_iter)?;
        let destination_token_program_info = next_account_info(account_info_iter)?;
        let pool_token_program_info = next_account_info(account_info_iter)?;
        let swap_curve_info = next_account_info(account_info_iter)?;

        if swap_info.owner != program_id {
            return Err(ProgramError::IncorrectProgramId);
        }
        let x: AccountLoader<SwapPool> = AccountLoader::try_from(swap_info)?;
        let token_swap = x.load()?;
        let swap_curve = curve!(swap_curve_info, token_swap);

        if *authority_info.key
            != Self::authority_id(program_id, swap_info.key, token_swap.bump_seed())?
        {
            return Err(SwapError::InvalidProgramAddress.into());
        }
        if !(*swap_source_info.key == *token_swap.token_a_account()
            || *swap_source_info.key == *token_swap.token_b_account())
        {
            return Err(SwapError::IncorrectSwapAccount.into());
        }
        if !(*swap_destination_info.key == *token_swap.token_a_account()
            || *swap_destination_info.key == *token_swap.token_b_account())
        {
            return Err(SwapError::IncorrectSwapAccount.into());
        }
        if *swap_source_info.key == *swap_destination_info.key {
            return Err(SwapError::InvalidInput.into());
        }
        if swap_source_info.key == source_info.key {
            return Err(SwapError::InvalidInput.into());
        }
        if swap_destination_info.key == destination_info.key {
            return Err(SwapError::InvalidInput.into());
        }
        if *pool_mint_info.key != *token_swap.pool_mint() {
            return Err(SwapError::IncorrectPoolMint.into());
        }
        if *pool_fee_account_info.key != *token_swap.pool_fee_account() {
            return Err(SwapError::IncorrectFeeAccount.into());
        }
        if *pool_token_program_info.key != *token_swap.token_program_id() {
            return Err(SwapError::IncorrectTokenProgramId.into());
        }

        let source_account =
            Self::unpack_token_account(swap_source_info, token_swap.token_program_id())?;
        let dest_account =
            Self::unpack_token_account(swap_destination_info, token_swap.token_program_id())?;
        let pool_mint = Self::unpack_mint(pool_mint_info, token_swap.token_program_id())?;

        // Take transfer fees into account for actual amount transferred in
        let actual_amount_in = {
            let source_mint_data = source_token_mint_info.data.borrow();
            let source_mint = Self::unpack_mint_with_extensions(
                &source_mint_data,
                source_token_mint_info.owner,
                token_swap.token_program_id(),
            )?;

            if let Ok(transfer_fee_config) = source_mint.get_extension::<TransferFeeConfig>() {
                amount_in.saturating_sub(
                    transfer_fee_config
                        .calculate_epoch_fee(Clock::get()?.epoch, amount_in)
                        .ok_or(SwapError::FeeCalculationFailure)?,
                )
            } else {
                amount_in
            }
        };

        // Calculate the trade amounts
        let trade_direction = if *swap_source_info.key == *token_swap.token_a_account() {
            TradeDirection::AtoB
        } else {
            TradeDirection::BtoA
        };
        let result = swap_curve
            .swap(
                to_u128(actual_amount_in)?,
                to_u128(source_account.amount)?,
                to_u128(dest_account.amount)?,
                trade_direction,
                token_swap.fees(),
            )
            .ok_or(SwapError::ZeroTradingTokens)?;

        // Re-calculate the source amount swapped based on what the curve says
        let (source_transfer_amount, source_mint_decimals) = {
            let source_amount_swapped = to_u64(result.source_amount_swapped)?;

            let source_mint_data = source_token_mint_info.data.borrow();
            let source_mint = Self::unpack_mint_with_extensions(
                &source_mint_data,
                source_token_mint_info.owner,
                token_swap.token_program_id(),
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
            let destination_mint_data = destination_token_mint_info.data.borrow();
            let destination_mint = Self::unpack_mint_with_extensions(
                &destination_mint_data,
                source_token_mint_info.owner,
                token_swap.token_program_id(),
            )?;
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

        Self::token_transfer(
            swap_info.key,
            source_token_program_info.clone(),
            source_info.clone(),
            source_token_mint_info.clone(),
            swap_source_info.clone(),
            user_transfer_authority_info.clone(),
            token_swap.bump_seed(),
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
                    to_u128(pool_mint.supply)?,
                    trade_direction,
                    RoundDirection::Floor,
                )
                .ok_or(SwapError::FeeCalculationFailure)?;
            // Allow error to fall through
            if let Ok(host_fee_account_info) = next_account_info(account_info_iter) {
                let host_fee_account = Self::unpack_token_account(
                    host_fee_account_info,
                    token_swap.token_program_id(),
                )?;
                if *pool_mint_info.key != host_fee_account.mint {
                    return Err(SwapError::IncorrectPoolMint.into());
                }
                let host_fee = token_swap
                    .fees()
                    .host_fee(pool_token_amount)
                    .ok_or(SwapError::FeeCalculationFailure)?;
                if host_fee > 0 {
                    pool_token_amount = pool_token_amount
                        .checked_sub(host_fee)
                        .ok_or(SwapError::FeeCalculationFailure)?;
                    Self::token_mint_to(
                        swap_info.key,
                        pool_token_program_info.clone(),
                        pool_mint_info.clone(),
                        host_fee_account_info.clone(),
                        authority_info.clone(),
                        token_swap.bump_seed(),
                        to_u64(host_fee)?,
                    )?;
                }
            }
            if token_swap
                .check_pool_fee_info(pool_fee_account_info)
                .is_ok()
            {
                Self::token_mint_to(
                    swap_info.key,
                    pool_token_program_info.clone(),
                    pool_mint_info.clone(),
                    pool_fee_account_info.clone(),
                    authority_info.clone(),
                    token_swap.bump_seed(),
                    to_u64(pool_token_amount)?,
                )?;
            };
        }

        Self::token_transfer(
            swap_info.key,
            destination_token_program_info.clone(),
            swap_destination_info.clone(),
            destination_token_mint_info.clone(),
            destination_info.clone(),
            authority_info.clone(),
            token_swap.bump_seed(),
            destination_transfer_amount,
            destination_mint_decimals,
        )?;

        Ok(())
    }

    /// Processes an [DepositAllTokenTypes](enum.Instruction.html).
    pub fn process_deposit_all_token_types(
        program_id: &Pubkey,
        pool_token_amount: u64,
        maximum_token_a_amount: u64,
        maximum_token_b_amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let swap_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let user_transfer_authority_info = next_account_info(account_info_iter)?;
        let source_a_info = next_account_info(account_info_iter)?;
        let source_b_info = next_account_info(account_info_iter)?;
        let token_a_info = next_account_info(account_info_iter)?;
        let token_b_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let dest_info = next_account_info(account_info_iter)?;
        let token_a_mint_info = next_account_info(account_info_iter)?;
        let token_b_mint_info = next_account_info(account_info_iter)?;
        let token_a_program_info = next_account_info(account_info_iter)?;
        let token_b_program_info = next_account_info(account_info_iter)?;
        let pool_token_program_info = next_account_info(account_info_iter)?;
        let swap_curve_info = next_account_info(account_info_iter)?;

        let x: AccountLoader<SwapPool> = AccountLoader::try_from(swap_info)?;
        let token_swap = x.load()?;
        let swap_curve = curve!(swap_curve_info, token_swap);

        let calculator = &swap_curve.calculator;
        if !calculator.allows_deposits() {
            return Err(SwapError::UnsupportedCurveOperation.into());
        }
        Self::check_accounts(
            &token_swap,
            program_id,
            swap_info,
            authority_info,
            token_a_info,
            token_b_info,
            pool_mint_info,
            pool_token_program_info,
            Some(source_a_info),
            Some(source_b_info),
            None,
        )?;

        let token_a = Self::unpack_token_account(token_a_info, token_swap.token_program_id())?;
        let token_b = Self::unpack_token_account(token_b_info, token_swap.token_program_id())?;
        let pool_mint = Self::unpack_mint(pool_mint_info, token_swap.token_program_id())?;
        let current_pool_mint_supply = to_u128(pool_mint.supply)?;
        let (pool_token_amount, pool_mint_supply) = if current_pool_mint_supply > 0 {
            (to_u128(pool_token_amount)?, current_pool_mint_supply)
        } else {
            (calculator.new_pool_supply(), calculator.new_pool_supply())
        };

        let results = calculator
            .pool_tokens_to_trading_tokens(
                pool_token_amount,
                pool_mint_supply,
                to_u128(token_a.amount)?,
                to_u128(token_b.amount)?,
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

        Self::token_transfer(
            swap_info.key,
            token_a_program_info.clone(),
            source_a_info.clone(),
            token_a_mint_info.clone(),
            token_a_info.clone(),
            user_transfer_authority_info.clone(),
            token_swap.bump_seed(),
            token_a_amount,
            Self::unpack_mint(token_a_mint_info, token_swap.token_program_id())?.decimals,
        )?;

        Self::token_transfer(
            swap_info.key,
            token_b_program_info.clone(),
            source_b_info.clone(),
            token_b_mint_info.clone(),
            token_b_info.clone(),
            user_transfer_authority_info.clone(),
            token_swap.bump_seed(),
            token_b_amount,
            Self::unpack_mint(token_b_mint_info, token_swap.token_program_id())?.decimals,
        )?;

        Self::token_mint_to(
            swap_info.key,
            pool_token_program_info.clone(),
            pool_mint_info.clone(),
            dest_info.clone(),
            authority_info.clone(),
            token_swap.bump_seed(),
            pool_token_amount,
        )?;

        Ok(())
    }

    /// Processes an [WithdrawAllTokenTypes](enum.Instruction.html).
    pub fn process_withdraw_all_token_types(
        program_id: &Pubkey,
        pool_token_amount: u64,
        minimum_token_a_amount: u64,
        minimum_token_b_amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let swap_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let user_transfer_authority_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let source_info = next_account_info(account_info_iter)?;
        let token_a_info = next_account_info(account_info_iter)?;
        let token_b_info = next_account_info(account_info_iter)?;
        let dest_token_a_info = next_account_info(account_info_iter)?;
        let dest_token_b_info = next_account_info(account_info_iter)?;
        let pool_fee_account_info = next_account_info(account_info_iter)?;
        let token_a_mint_info = next_account_info(account_info_iter)?;
        let token_b_mint_info = next_account_info(account_info_iter)?;
        let pool_token_program_info = next_account_info(account_info_iter)?;
        let token_a_program_info = next_account_info(account_info_iter)?;
        let token_b_program_info = next_account_info(account_info_iter)?;
        let swap_curve_info = next_account_info(account_info_iter)?;

        let x: AccountLoader<SwapPool> = AccountLoader::try_from(swap_info)?;
        let token_swap = x.load()?;
        let swap_curve = curve!(swap_curve_info, token_swap);

        Self::check_accounts(
            &token_swap,
            program_id,
            swap_info,
            authority_info,
            token_a_info,
            token_b_info,
            pool_mint_info,
            pool_token_program_info,
            Some(dest_token_a_info),
            Some(dest_token_b_info),
            Some(pool_fee_account_info),
        )?;

        let token_a = Self::unpack_token_account(token_a_info, token_swap.token_program_id())?;
        let token_b = Self::unpack_token_account(token_b_info, token_swap.token_program_id())?;
        let pool_mint = Self::unpack_mint(pool_mint_info, token_swap.token_program_id())?;

        let calculator = &swap_curve.calculator;

        let withdraw_fee = match token_swap.check_pool_fee_info(pool_fee_account_info) {
            Ok(_) => {
                if *pool_fee_account_info.key == *source_info.key {
                    // withdrawing from the fee account, don't assess withdraw fee
                    0
                } else {
                    token_swap
                        .fees()
                        .owner_withdraw_fee(to_u128(pool_token_amount)?)
                        .ok_or(SwapError::FeeCalculationFailure)?
                }
            }
            Err(_) => 0,
        };
        let pool_token_amount = to_u128(pool_token_amount)?
            .checked_sub(withdraw_fee)
            .ok_or(SwapError::CalculationFailure)?;

        let results = calculator
            .pool_tokens_to_trading_tokens(
                pool_token_amount,
                to_u128(pool_mint.supply)?,
                to_u128(token_a.amount)?,
                to_u128(token_b.amount)?,
                RoundDirection::Floor,
            )
            .ok_or(SwapError::ZeroTradingTokens)?;
        let token_a_amount = to_u64(results.token_a_amount)?;
        let token_a_amount = std::cmp::min(token_a.amount, token_a_amount);
        if token_a_amount < minimum_token_a_amount {
            msg!(
                "ExceededSlippage: token_a_amount={} < minimum_token_a_amount={}",
                token_a_amount,
                minimum_token_a_amount
            );
            return Err(SwapError::ExceededSlippage.into());
        }
        if token_a_amount == 0 && token_a.amount != 0 {
            return Err(SwapError::ZeroTradingTokens.into());
        }
        let token_b_amount = to_u64(results.token_b_amount)?;
        let token_b_amount = std::cmp::min(token_b.amount, token_b_amount);
        if token_b_amount < minimum_token_b_amount {
            msg!(
                "ExceededSlippage: token_b_amount={} < minimum_token_b_amount={}",
                token_b_amount,
                minimum_token_b_amount
            );
            return Err(SwapError::ExceededSlippage.into());
        }
        if token_b_amount == 0 && token_b.amount != 0 {
            return Err(SwapError::ZeroTradingTokens.into());
        }

        if withdraw_fee > 0 {
            Self::token_transfer(
                swap_info.key,
                pool_token_program_info.clone(),
                source_info.clone(),
                pool_mint_info.clone(),
                pool_fee_account_info.clone(),
                user_transfer_authority_info.clone(),
                token_swap.bump_seed(),
                to_u64(withdraw_fee)?,
                pool_mint.decimals,
            )?;
        }
        Self::token_burn(
            swap_info.key,
            pool_token_program_info.clone(),
            source_info.clone(),
            pool_mint_info.clone(),
            user_transfer_authority_info.clone(),
            token_swap.bump_seed(),
            to_u64(pool_token_amount)?,
        )?;

        if token_a_amount > 0 {
            Self::token_transfer(
                swap_info.key,
                token_a_program_info.clone(),
                token_a_info.clone(),
                token_a_mint_info.clone(),
                dest_token_a_info.clone(),
                authority_info.clone(),
                token_swap.bump_seed(),
                token_a_amount,
                Self::unpack_mint(token_a_mint_info, token_swap.token_program_id())?.decimals,
            )?;
        }
        if token_b_amount > 0 {
            Self::token_transfer(
                swap_info.key,
                token_b_program_info.clone(),
                token_b_info.clone(),
                token_b_mint_info.clone(),
                dest_token_b_info.clone(),
                authority_info.clone(),
                token_swap.bump_seed(),
                token_b_amount,
                Self::unpack_mint(token_b_mint_info, token_swap.token_program_id())?.decimals,
            )?;
        }
        Ok(())
    }

    /// Processes DepositSingleTokenTypeExactAmountIn
    pub fn process_deposit_single_token_type_exact_amount_in(
        program_id: &Pubkey,
        source_token_amount: u64,
        minimum_pool_token_amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let swap_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let user_transfer_authority_info = next_account_info(account_info_iter)?;
        let source_info = next_account_info(account_info_iter)?;
        let swap_token_a_info = next_account_info(account_info_iter)?;
        let swap_token_b_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let destination_info = next_account_info(account_info_iter)?;
        let source_token_mint_info = next_account_info(account_info_iter)?;
        let source_token_program_info = next_account_info(account_info_iter)?;
        let pool_token_program_info = next_account_info(account_info_iter)?;
        let swap_curve_info = next_account_info(account_info_iter)?;

        let x: AccountLoader<SwapPool> = AccountLoader::try_from(swap_info)?;
        let token_swap = x.load()?;
        let swap_curve = curve!(swap_curve_info, token_swap);

        let calculator = &swap_curve.calculator;
        if !calculator.allows_deposits() {
            return Err(SwapError::UnsupportedCurveOperation.into());
        }
        let source_account =
            Self::unpack_token_account(source_info, token_swap.token_program_id())?;
        let swap_token_a =
            Self::unpack_token_account(swap_token_a_info, token_swap.token_program_id())?;
        let swap_token_b =
            Self::unpack_token_account(swap_token_b_info, token_swap.token_program_id())?;

        let trade_direction = if source_account.mint == swap_token_a.mint {
            TradeDirection::AtoB
        } else if source_account.mint == swap_token_b.mint {
            TradeDirection::BtoA
        } else {
            return Err(SwapError::IncorrectSwapAccount.into());
        };

        let (source_a_info, source_b_info) = match trade_direction {
            TradeDirection::AtoB => (Some(source_info), None),
            TradeDirection::BtoA => (None, Some(source_info)),
        };

        Self::check_accounts(
            &token_swap,
            program_id,
            swap_info,
            authority_info,
            swap_token_a_info,
            swap_token_b_info,
            pool_mint_info,
            pool_token_program_info,
            source_a_info,
            source_b_info,
            None,
        )?;

        let pool_mint = Self::unpack_mint(pool_mint_info, token_swap.token_program_id())?;
        let pool_mint_supply = to_u128(pool_mint.supply)?;
        let pool_token_amount = if pool_mint_supply > 0 {
            swap_curve
                .deposit_single_token_type(
                    to_u128(source_token_amount)?,
                    to_u128(swap_token_a.amount)?,
                    to_u128(swap_token_b.amount)?,
                    pool_mint_supply,
                    trade_direction,
                    token_swap.fees(),
                )
                .ok_or(SwapError::ZeroTradingTokens)?
        } else {
            calculator.new_pool_supply()
        };

        let pool_token_amount = to_u64(pool_token_amount)?;
        if pool_token_amount < minimum_pool_token_amount {
            return Err(SwapError::ExceededSlippage.into());
        }
        if pool_token_amount == 0 {
            return Err(SwapError::ZeroTradingTokens.into());
        }

        match trade_direction {
            TradeDirection::AtoB => {
                Self::token_transfer(
                    swap_info.key,
                    source_token_program_info.clone(),
                    source_info.clone(),
                    source_token_mint_info.clone(),
                    swap_token_a_info.clone(),
                    user_transfer_authority_info.clone(),
                    token_swap.bump_seed(),
                    source_token_amount,
                    Self::unpack_mint(source_token_mint_info, token_swap.token_program_id())?
                        .decimals,
                )?;
            }
            TradeDirection::BtoA => {
                Self::token_transfer(
                    swap_info.key,
                    source_token_program_info.clone(),
                    source_info.clone(),
                    source_token_mint_info.clone(),
                    swap_token_b_info.clone(),
                    user_transfer_authority_info.clone(),
                    token_swap.bump_seed(),
                    source_token_amount,
                    Self::unpack_mint(source_token_mint_info, token_swap.token_program_id())?
                        .decimals,
                )?;
            }
        }

        Self::token_mint_to(
            swap_info.key,
            pool_token_program_info.clone(),
            pool_mint_info.clone(),
            destination_info.clone(),
            authority_info.clone(),
            token_swap.bump_seed(),
            pool_token_amount,
        )?;

        Ok(())
    }

    /// Processes a [WithdrawSingleTokenTypeExactAmountOut](enum.Instruction.html).
    pub fn process_withdraw_single_token_type_exact_amount_out(
        program_id: &Pubkey,
        destination_token_amount: u64,
        maximum_pool_token_amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let swap_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let user_transfer_authority_info = next_account_info(account_info_iter)?;
        let pool_mint_info = next_account_info(account_info_iter)?;
        let source_info = next_account_info(account_info_iter)?;
        let swap_token_a_info = next_account_info(account_info_iter)?;
        let swap_token_b_info = next_account_info(account_info_iter)?;
        let destination_info = next_account_info(account_info_iter)?;
        let pool_fee_account_info = next_account_info(account_info_iter)?;
        let destination_token_mint_info = next_account_info(account_info_iter)?;
        let pool_token_program_info = next_account_info(account_info_iter)?;
        let destination_token_program_info = next_account_info(account_info_iter)?;
        let swap_curve_info = next_account_info(account_info_iter)?;

        let x: AccountLoader<SwapPool> = AccountLoader::try_from(swap_info)?;
        let token_swap = x.load()?;
        let swap_curve = curve!(swap_curve_info, token_swap);

        let destination_account =
            Self::unpack_token_account(destination_info, token_swap.token_program_id())?;
        let swap_token_a =
            Self::unpack_token_account(swap_token_a_info, token_swap.token_program_id())?;
        let swap_token_b =
            Self::unpack_token_account(swap_token_b_info, token_swap.token_program_id())?;

        let trade_direction = if destination_account.mint == swap_token_a.mint {
            TradeDirection::AtoB
        } else if destination_account.mint == swap_token_b.mint {
            TradeDirection::BtoA
        } else {
            return Err(SwapError::IncorrectSwapAccount.into());
        };

        let (destination_a_info, destination_b_info) = match trade_direction {
            TradeDirection::AtoB => (Some(destination_info), None),
            TradeDirection::BtoA => (None, Some(destination_info)),
        };
        Self::check_accounts(
            &token_swap,
            program_id,
            swap_info,
            authority_info,
            swap_token_a_info,
            swap_token_b_info,
            pool_mint_info,
            pool_token_program_info,
            destination_a_info,
            destination_b_info,
            Some(pool_fee_account_info),
        )?;

        let pool_mint = Self::unpack_mint(pool_mint_info, token_swap.token_program_id())?;
        let pool_mint_supply = to_u128(pool_mint.supply)?;
        let swap_token_a_amount = to_u128(swap_token_a.amount)?;
        let swap_token_b_amount = to_u128(swap_token_b.amount)?;

        let burn_pool_token_amount = swap_curve
            .withdraw_single_token_type_exact_out(
                to_u128(destination_token_amount)?,
                swap_token_a_amount,
                swap_token_b_amount,
                pool_mint_supply,
                trade_direction,
                token_swap.fees(),
            )
            .ok_or(SwapError::ZeroTradingTokens)?;

        let withdraw_fee = match token_swap.check_pool_fee_info(pool_fee_account_info) {
            Ok(_) => {
                if *pool_fee_account_info.key == *source_info.key {
                    // withdrawing from the fee account, don't assess withdraw fee
                    0
                } else {
                    token_swap
                        .fees()
                        .owner_withdraw_fee(burn_pool_token_amount)
                        .ok_or(SwapError::FeeCalculationFailure)?
                }
            }
            Err(_) => 0,
        };
        let pool_token_amount = burn_pool_token_amount
            .checked_add(withdraw_fee)
            .ok_or(SwapError::CalculationFailure)?;

        if to_u64(pool_token_amount)? > maximum_pool_token_amount {
            return Err(SwapError::ExceededSlippage.into());
        }
        if pool_token_amount == 0 {
            return Err(SwapError::ZeroTradingTokens.into());
        }

        if withdraw_fee > 0 {
            Self::token_transfer(
                swap_info.key,
                pool_token_program_info.clone(),
                source_info.clone(),
                pool_mint_info.clone(),
                pool_fee_account_info.clone(),
                user_transfer_authority_info.clone(),
                token_swap.bump_seed(),
                to_u64(withdraw_fee)?,
                pool_mint.decimals,
            )?;
        }
        Self::token_burn(
            swap_info.key,
            pool_token_program_info.clone(),
            source_info.clone(),
            pool_mint_info.clone(),
            user_transfer_authority_info.clone(),
            token_swap.bump_seed(),
            to_u64(burn_pool_token_amount)?,
        )?;

        match trade_direction {
            TradeDirection::AtoB => {
                Self::token_transfer(
                    swap_info.key,
                    destination_token_program_info.clone(),
                    swap_token_a_info.clone(),
                    destination_token_mint_info.clone(),
                    destination_info.clone(),
                    authority_info.clone(),
                    token_swap.bump_seed(),
                    destination_token_amount,
                    Self::unpack_mint(destination_token_mint_info, token_swap.token_program_id())?
                        .decimals,
                )?;
            }
            TradeDirection::BtoA => {
                Self::token_transfer(
                    swap_info.key,
                    destination_token_program_info.clone(),
                    swap_token_b_info.clone(),
                    destination_token_mint_info.clone(),
                    destination_info.clone(),
                    authority_info.clone(),
                    token_swap.bump_seed(),
                    destination_token_amount,
                    Self::unpack_mint(destination_token_mint_info, token_swap.token_program_id())?
                        .decimals,
                )?;
            }
        }

        Ok(())
    }

    /// Processes an [Instruction](enum.Instruction.html).
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        Self::process_with_constraints(program_id, accounts, input, &SWAP_CONSTRAINTS)
    }

    /// Processes an instruction given extra constraint
    pub fn process_with_constraints(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        input: &[u8],
        _swap_constraints: &Option<SwapConstraints>, // todo - elliot - compile time constraints
    ) -> ProgramResult {
        let instruction = SwapInstruction::unpack(input)?;
        match instruction {
            SwapInstruction::InitializePool {
                fees: _fees,
                curve: _swap_curve,
                initial_supply_a: _initial_supply_a,
                initial_supply_b: _initial_supply_b,
            } => {
                msg!("Instruction: InitializePool");
                crate::entry(program_id, accounts, input)
            }
            SwapInstruction::Swap {
                amount_in: _amount_in,
                minimum_amount_out: _minimum_amount_out,
            } => {
                msg!("Instruction: Swap");
                crate::entry(program_id, accounts, input)
            }
            SwapInstruction::DepositAllTokenTypes {
                pool_token_amount: _pool_token_amount,
                maximum_token_a_amount: _maximum_token_a_amount,
                maximum_token_b_amount: _maximum_token_b_amount,
            } => {
                msg!("Instruction: DepositAllTokenTypes");
                crate::entry(program_id, accounts, input)
            }
            SwapInstruction::WithdrawAllTokenTypes {
                pool_token_amount,
                minimum_token_a_amount,
                minimum_token_b_amount,
            } => {
                msg!("Instruction: WithdrawAllTokenTypes");
                Self::process_withdraw_all_token_types(
                    program_id,
                    pool_token_amount,
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                    accounts,
                )
            }
            SwapInstruction::DepositSingleTokenTypeExactAmountIn {
                source_token_amount,
                minimum_pool_token_amount,
            } => {
                msg!("Instruction: DepositSingleTokenTypeExactAmountIn");
                Self::process_deposit_single_token_type_exact_amount_in(
                    program_id,
                    source_token_amount,
                    minimum_pool_token_amount,
                    accounts,
                )
            }
            SwapInstruction::WithdrawSingleTokenTypeExactAmountOut {
                destination_token_amount,
                maximum_pool_token_amount,
            } => {
                msg!("Instruction: WithdrawSingleTokenTypeExactAmountOut");
                Self::process_withdraw_single_token_type_exact_amount_out(
                    program_id,
                    destination_token_amount,
                    maximum_pool_token_amount,
                    accounts,
                )
            }
        }
    }
}

fn invoke_signed_wrapper<T>(
    instruction: &Instruction,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> Result<(), ProgramError>
where
    T: 'static + PrintProgramError + DecodeError<T> + FromPrimitive + Error,
{
    invoke_signed(instruction, account_infos, signers_seeds).map_err(|err| {
        err.print::<T>();
        err
    })
}

#[cfg(test)]
mod tests {

    use anchor_lang::solana_program::{
        clock::Clock, entrypoint::SUCCESS, instruction::Instruction, program_pack::Pack,
        program_stubs, rent::Rent,
    };
    use anchor_lang::AccountDeserialize;
    use anchor_lang::{error::ErrorCode as AnchorError, solana_program};
    use solana_sdk::account::{
        create_account_for_test, Account as SolanaAccount, ReadableAccount, WritableAccount,
    };
    use spl_token_2022::{
        error::TokenError,
        extension::{
            transfer_fee::{instruction::initialize_transfer_fee_config, TransferFee},
            ExtensionType,
        },
        instruction::{
            approve, close_account, freeze_account, initialize_account, initialize_immutable_owner,
            initialize_mint, initialize_mint_close_authority, mint_to,
        },
    };
    use test_case::test_case;

    use crate::instructions::CurveParameters;
    use crate::{
        curve::base::CurveType, curve::calculator::INITIAL_SWAP_POOL_AMOUNT, ix, InitialSupply,
    };

    use super::*;

    struct TestSyscallStubs {}
    impl program_stubs::SyscallStubs for TestSyscallStubs {
        fn sol_invoke_signed(
            &self,
            instruction: &Instruction,
            account_infos: &[AccountInfo],
            signers_seeds: &[&[&[u8]]],
        ) -> ProgramResult {
            let mut account_infos_ordered = vec![];

            msg!("TestSyscallStubs::sol_invoke_signed()");

            // order account infos as the instruction expects them as defined in the account_metas
            // re-add signer flag if signer
            for meta in instruction.accounts.iter() {
                for account_info in account_infos.iter() {
                    if meta.pubkey == *account_info.key {
                        let mut new_account_info = account_info.clone();
                        for seeds in signers_seeds.iter() {
                            msg!("TestSyscallStubs::sol_invoke_signed() seeds: {:?}", seeds);
                            let signer =
                                Pubkey::create_program_address(seeds, &crate::id()).unwrap();
                            if *account_info.key == signer {
                                new_account_info.is_signer = true;
                            }
                        }
                        account_infos_ordered.push(new_account_info);
                    }
                }
            }

            if instruction.program_id == spl_token::id() {
                msg!("sol_invoke_signed: token program id");
                spl_token::processor::Processor::process(
                    &instruction.program_id,
                    &account_infos_ordered,
                    &instruction.data,
                )?; // NOTE: unwrap here to get a stack trace
            } else if instruction.program_id == spl_token_2022::id() {
                msg!("sol_invoke_signed: token 2022 program id");
                spl_token_2022::processor::Processor::process(
                    &instruction.program_id,
                    &account_infos_ordered,
                    &instruction.data,
                )?; // NOTE: unwrap here to get a stack trace
            } else if instruction.program_id == solana_program::system_program::id() {
                // https://github.com/solana-labs/solana/blob/master/runtime/src/system_instruction_processor.rs
                // we have the system program defined in the master/runtime of the main repo
                msg!("sol_invoke_signed: system program id");
                msg!("ix: {:?}", instruction);
            } else {
                unreachable!("sol_invoke_signed: unhandled program_id");
            }

            Ok(())
        }

        fn sol_get_clock_sysvar(&self, var_addr: *mut u8) -> u64 {
            unsafe {
                *(var_addr as *mut _ as *mut Clock) = Clock::default();
            }
            SUCCESS
        }

        fn sol_get_rent_sysvar(&self, var_addr: *mut u8) -> u64 {
            unsafe {
                *(var_addr as *mut _ as *mut Rent) = Rent::default();
            }
            SUCCESS
        }
    }

    fn test_syscall_stubs() {
        use std::sync::Once;
        static ONCE: Once = Once::new();

        ONCE.call_once(|| {
            program_stubs::set_syscall_stubs(Box::new(TestSyscallStubs {}));
        });
    }

    // todo - xfer fees
    #[derive(Default)]
    struct SwapTransferFees {
        _pool_token: TransferFee,
        token_a: TransferFee,
        token_b: TransferFee,
    }

    struct SwapAccountInfo {
        admin_authority: Pubkey,
        pool_authority_bump_seed: u8,
        pool_authority: Pubkey,
        fees: Fees,
        initial_supply: InitialSupply,
        transfer_fees: SwapTransferFees,
        pool: Pubkey,
        pool_account: SolanaAccount,
        swap_curve_key: Pubkey,
        swap_curve_account: SolanaAccount,
        swap_curve: SwapCurve,
        curve_params: CurveParameters,
        pool_token_mint_key: Pubkey,
        pool_token_mint_account: SolanaAccount,
        pool_token_fees_vault_key: Pubkey,
        pool_token_fees_vault_account: SolanaAccount,
        admin_authority_token_a_ata_key: Pubkey,
        admin_authority_token_a_ata_account: SolanaAccount,
        admin_authority_token_b_ata_key: Pubkey,
        admin_authority_token_b_ata_account: SolanaAccount,
        admin_authority_pool_token_ata_key: Pubkey,
        admin_authority_pool_token_ata_account: SolanaAccount,
        token_a_vault_key: Pubkey,
        token_a_vault_account: SolanaAccount,
        token_a_mint_key: Pubkey,
        token_a_mint_account: SolanaAccount,
        token_b_vault_key: Pubkey,
        token_b_vault_account: SolanaAccount,
        token_b_mint_key: Pubkey,
        token_b_mint_account: SolanaAccount,
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    }

    impl SwapAccountInfo {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            admin_authority: &Pubkey,
            fees: Fees,
            transfer_fees: SwapTransferFees,
            curve_params: CurveParameters,
            initial_supply: InitialSupply,
            pool_token_program_id: &Pubkey,
            token_a_program_id: &Pubkey,
            token_b_program_id: &Pubkey,
        ) -> Self {
            let InitialSupply {
                initial_supply_a,
                initial_supply_b,
            } = initial_supply;
            let pool = Pubkey::new_unique();
            let pool_account = SolanaAccount::new(u32::MAX as u64, SwapPool::LEN, &crate::id());
            let (swap_curve_key, _swap_curve_bump_seed) =
                Pubkey::find_program_address(&[b"curve".as_ref(), pool.as_ref()], &crate::id());
            let swap_curve_account =
                SolanaAccount::new(u32::MAX as u64, crate::state::Curve::LEN, &crate::id());
            let (pool_authority, pool_authority_bump_seed) = Pubkey::find_program_address(
                &[b"pauthority".as_ref(), pool.as_ref()],
                &crate::id(),
            );

            let (pool_token_mint_key, _pool_token_mint_bump_seed) =
                Pubkey::find_program_address(&[b"lp".as_ref(), pool.as_ref()], &crate::id());

            let pool_token_mint_account = SolanaAccount::new(
                u32::MAX as u64,
                spl_token_2022::state::Mint::LEN,
                pool_token_program_id, // todo - this should be system but we no-op the system program calls
            );

            let admin_authority_pool_token_ata_key = Pubkey::new_unique();
            let admin_authority_pool_token_ata_account = SolanaAccount::new(
                u32::MAX as u64,
                spl_token_2022::state::Account::LEN,
                pool_token_program_id, // todo - this should be system but we no-op the system program calls
            );

            let (pool_token_fees_vault_key, _pool_token_fees_vault_bump_seed) =
                Pubkey::find_program_address(
                    &[
                        b"lpfee".as_ref(),
                        pool.as_ref(),
                        pool_token_mint_key.as_ref(),
                    ],
                    &crate::id(),
                );
            let pool_token_fees_vault_account = SolanaAccount::new(
                u32::MAX as u64,
                spl_token_2022::state::Account::LEN,
                pool_token_program_id, // todo - this should be system but we no-op the system program calls
            );

            let (token_a_mint_key, mut token_a_mint_account) = create_mint(
                token_a_program_id,
                admin_authority,
                None,
                None,
                &transfer_fees.token_a,
            );
            let (token_a_vault_key, _token_a_vault_bump_seed) = Pubkey::find_program_address(
                &[
                    b"pvault_a".as_ref(),
                    pool.as_ref(),
                    token_a_mint_key.as_ref(),
                ],
                &crate::id(),
            );
            let token_a_vault_account = SolanaAccount::new(
                u32::MAX as u64,
                get_token_account_space(token_a_program_id, &token_a_mint_account), // todo size needed because syscall not stubbed
                token_a_program_id, // todo - this should be system but we no-op the system program calls
            );
            let (admin_authority_token_a_ata_key, admin_authority_token_a_ata_account) =
                create_token_account(
                    token_a_program_id,
                    &token_a_mint_key,
                    &mut token_a_mint_account,
                    admin_authority,
                    admin_authority,
                    initial_supply_a,
                );

            let (token_b_mint_key, mut token_b_mint_account) = create_mint(
                token_b_program_id,
                admin_authority,
                None,
                None,
                &transfer_fees.token_b,
            );
            let (token_b_vault_key, _token_b_vault_bump_seed) = Pubkey::find_program_address(
                &[
                    b"pvault_b".as_ref(),
                    pool.as_ref(),
                    token_b_mint_key.as_ref(),
                ],
                &crate::id(),
            );
            let token_b_vault_account = SolanaAccount::new(
                u32::MAX as u64,
                get_token_account_space(token_b_program_id, &token_b_mint_account), // todo size needed because syscall not stubbed
                token_b_program_id, // todo - this should be system but we no-op the system program calls
            );
            let (admin_authority_token_b_ata_key, admin_authority_token_b_ata_account) =
                create_token_account(
                    token_b_program_id,
                    &token_b_mint_key,
                    &mut token_b_mint_account,
                    admin_authority,
                    admin_authority,
                    initial_supply_b,
                );

            SwapAccountInfo {
                admin_authority: *admin_authority,
                pool_authority_bump_seed,
                pool_authority,
                fees,
                initial_supply,
                transfer_fees,
                pool,
                pool_account,
                swap_curve_key,
                swap_curve_account,
                swap_curve: SwapCurve::new_from_params(curve_params.clone()),
                curve_params,
                pool_token_mint_key,
                pool_token_mint_account,
                pool_token_fees_vault_key,
                pool_token_fees_vault_account,
                admin_authority_token_a_ata_key,
                admin_authority_token_a_ata_account,
                admin_authority_token_b_ata_key,
                admin_authority_token_b_ata_account,
                admin_authority_pool_token_ata_key,
                admin_authority_pool_token_ata_account,
                token_a_vault_key,
                token_a_vault_account,
                token_a_mint_key,
                token_a_mint_account,
                token_b_vault_key,
                token_b_vault_account,
                token_b_mint_key,
                token_b_mint_account,
                pool_token_program_id: *pool_token_program_id,
                token_a_program_id: *token_a_program_id,
                token_b_program_id: *token_b_program_id,
            }
        }

        pub fn initialize_pool(&mut self) -> ProgramResult {
            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);
            do_process_instruction(
                ix::initialize_pool(
                    &crate::id(),
                    &self.admin_authority,
                    &self.pool,
                    &self.swap_curve_key,
                    &self.token_a_mint_key,
                    &self.token_b_mint_key,
                    &self.token_a_vault_key,
                    &self.token_b_vault_key,
                    &self.pool_authority,
                    &self.pool_token_mint_key,
                    &self.pool_token_fees_vault_key,
                    &self.admin_authority_token_a_ata_key,
                    &self.admin_authority_token_b_ata_key,
                    &self.admin_authority_pool_token_ata_key,
                    &self.pool_token_program_id,
                    &self.token_a_program_id,
                    &self.token_b_program_id,
                    self.fees,
                    self.initial_supply.clone(),
                    self.curve_params.clone(),
                )
                .unwrap(),
                vec![
                    &mut SolanaAccount::default(),
                    &mut self.pool_account,
                    &mut self.swap_curve_account,
                    &mut SolanaAccount::default(),
                    &mut self.token_a_mint_account,
                    &mut self.token_b_mint_account,
                    &mut self.token_a_vault_account,
                    &mut self.token_b_vault_account,
                    &mut self.pool_token_mint_account,
                    &mut self.pool_token_fees_vault_account,
                    &mut self.admin_authority_token_a_ata_account,
                    &mut self.admin_authority_token_b_ata_account,
                    &mut self.admin_authority_pool_token_ata_account,
                    &mut exe.clone(), // system_program
                    &mut create_account_for_test(&Rent::default()),
                    &mut exe.clone(), // pool_token_program
                    &mut exe.clone(), // token_a_program
                    &mut exe.clone(), // token_b_program
                ],
            )
        }

        pub fn setup_token_accounts(
            &mut self,
            mint_owner: &Pubkey,
            account_owner: &Pubkey,
            a_amount: u64,
            b_amount: u64,
            pool_amount: u64,
        ) -> (
            Pubkey,
            SolanaAccount,
            Pubkey,
            SolanaAccount,
            Pubkey,
            SolanaAccount,
        ) {
            let (token_a_key, token_a_account) = create_token_account(
                &self.token_a_program_id,
                &self.token_a_mint_key,
                &mut self.token_a_mint_account,
                mint_owner,
                account_owner,
                a_amount,
            );
            let (token_b_key, token_b_account) = create_token_account(
                &self.token_b_program_id,
                &self.token_b_mint_key,
                &mut self.token_b_mint_account,
                mint_owner,
                account_owner,
                b_amount,
            );
            let (pool_key, pool_account) = create_token_account(
                &self.pool_token_program_id,
                &self.pool_token_mint_key,
                &mut self.pool_token_mint_account,
                &self.pool_authority,
                account_owner,
                pool_amount,
            );
            (
                token_a_key,
                token_a_account,
                token_b_key,
                token_b_account,
                pool_key,
                pool_account,
            )
        }

        fn get_swap_key(&self, mint_key: &Pubkey) -> &Pubkey {
            if *mint_key == self.token_a_mint_key {
                &self.token_a_vault_key
            } else if *mint_key == self.token_b_mint_key {
                &self.token_b_vault_key
            } else {
                panic!("Could not find matching swap token account");
            }
        }

        fn get_token_program_id(&self, account_key: &Pubkey) -> &Pubkey {
            if *account_key == self.token_a_vault_key {
                &self.token_a_program_id
            } else if *account_key == self.token_b_vault_key {
                &self.token_b_program_id
            } else {
                panic!("Could not find matching swap token account");
            }
        }

        fn get_token_mint(&self, account_key: &Pubkey) -> (Pubkey, SolanaAccount) {
            if *account_key == self.token_a_vault_key {
                (self.token_a_mint_key, self.token_a_mint_account.clone())
            } else if *account_key == self.token_b_vault_key {
                (self.token_b_mint_key, self.token_b_mint_account.clone())
            } else {
                panic!("Could not find matching swap token account");
            }
        }

        fn get_token_account(&self, account_key: &Pubkey) -> &SolanaAccount {
            if *account_key == self.token_a_vault_key {
                &self.token_a_vault_account
            } else if *account_key == self.token_b_vault_key {
                &self.token_b_vault_account
            } else {
                panic!("Could not find matching swap token account");
            }
        }

        fn set_token_account(&mut self, account_key: &Pubkey, account: SolanaAccount) {
            if *account_key == self.token_a_vault_key {
                self.token_a_vault_account = account;
                return;
            } else if *account_key == self.token_b_vault_key {
                self.token_b_vault_account = account;
                return;
            }
            panic!("Could not find matching swap token account");
        }

        #[allow(clippy::too_many_arguments)]
        pub fn swap(
            &mut self,
            user_key: &Pubkey,
            user_source_key: &Pubkey,
            user_source_account: &mut SolanaAccount,
            swap_source_key: &Pubkey,
            swap_destination_key: &Pubkey,
            user_destination_key: &Pubkey,
            user_destination_account: &mut SolanaAccount,
            amount_in: u64,
            minimum_amount_out: u64,
        ) -> ProgramResult {
            // let user_transfer_key = Pubkey::new_unique();
            let source_token_program_id = self.get_token_program_id(swap_source_key);
            let destination_token_program_id = self.get_token_program_id(swap_destination_key);
            // approve moving from user source account
            // todo - elliot - delegation
            // do_process_instruction(
            //     approve(
            //         source_token_program_id,
            //         user_source_key,
            //         &user_transfer_key,
            //         user_key,
            //         &[],
            //         amount_in,
            //     )
            //     .unwrap(),
            //     vec![
            //         user_source_account,
            //         &mut SolanaAccount::default(),
            //         &mut SolanaAccount::default(),
            //     ],
            // )
            // .unwrap();

            let (source_mint_key, mut source_mint_account) = self.get_token_mint(swap_source_key);
            let (destination_mint_key, mut destination_mint_account) =
                self.get_token_mint(swap_destination_key);
            let mut swap_source_account = self.get_token_account(swap_source_key).clone();
            let mut swap_destination_account = self.get_token_account(swap_destination_key).clone();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            // perform the swap
            do_process_instruction(
                ix::swap(
                    &crate::id(),
                    source_token_program_id,
                    destination_token_program_id,
                    &self.pool_token_program_id,
                    &self.pool,
                    &self.pool_authority,
                    user_key, // todo - elliot -delegation
                    user_source_key,
                    swap_source_key,
                    swap_destination_key,
                    user_destination_key,
                    &self.pool_token_mint_key,
                    &self.pool_token_fees_vault_key,
                    &source_mint_key,
                    &destination_mint_key,
                    &self.swap_curve_key,
                    None,
                    ix::Swap {
                        amount_in,
                        minimum_amount_out,
                    },
                )
                .unwrap(),
                vec![
                    &mut SolanaAccount::default(),
                    &mut self.pool_account,
                    &mut self.swap_curve_account,
                    &mut SolanaAccount::default(),
                    &mut source_mint_account,
                    &mut destination_mint_account,
                    &mut swap_source_account,
                    &mut swap_destination_account,
                    &mut self.pool_token_mint_account,
                    &mut self.pool_token_fees_vault_account,
                    user_source_account,
                    user_destination_account,
                    &mut exe.clone(), // Optional front end host fees - passed as the program if not present
                    &mut exe.clone(), // pool_token_program
                    &mut exe.clone(), // source_token_program
                    &mut exe.clone(), // destination_token_program
                ],
            )?;

            self.set_token_account(swap_source_key, swap_source_account);
            self.set_token_account(swap_destination_key, swap_destination_account);

            Ok(())
        }

        #[allow(clippy::too_many_arguments)]
        pub fn deposit_all_token_types(
            &mut self,
            depositor_key: &Pubkey,
            depositor_token_a_key: &Pubkey,
            depositor_token_a_account: &mut SolanaAccount,
            depositor_token_b_key: &Pubkey,
            depositor_token_b_account: &mut SolanaAccount,
            depositor_pool_key: &Pubkey,
            depositor_pool_account: &mut SolanaAccount,
            pool_token_amount: u64,
            maximum_token_a_amount: u64,
            maximum_token_b_amount: u64,
        ) -> ProgramResult {
            // let user_transfer_authority = Pubkey::new_unique();
            let token_a_program_id = depositor_token_a_account.owner;
            // todo - elliot - delegation
            // do_process_instruction(
            //     approve(
            //         &token_a_program_id,
            //         depositor_token_a_key,
            //         &user_transfer_authority,
            //         depositor_key,
            //         &[],
            //         maximum_token_a_amount,
            //     )
            //     .unwrap(),
            //     vec![
            //         depositor_token_a_account,
            //         &mut SolanaAccount::default(),
            //         &mut SolanaAccount::default(),
            //     ],
            // )
            // .unwrap();

            let token_b_program_id = depositor_token_b_account.owner;
            // todo - elliot - delegation
            // do_process_instruction(
            //     approve(
            //         &token_b_program_id,
            //         depositor_token_b_key,
            //         &user_transfer_authority,
            //         depositor_key,
            //         &[],
            //         maximum_token_b_amount,
            //     )
            //     .unwrap(),
            //     vec![
            //         depositor_token_b_account,
            //         &mut SolanaAccount::default(),
            //         &mut SolanaAccount::default(),
            //     ],
            // )
            // .unwrap();

            let pool_token_program_id = depositor_pool_account.owner;

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            do_process_instruction(
                ix::deposit_all_token_types(
                    &crate::id(),
                    &token_a_program_id,
                    &token_b_program_id,
                    &pool_token_program_id,
                    &self.pool,
                    &self.pool_authority,
                    depositor_key,
                    depositor_token_a_key,
                    depositor_token_b_key,
                    &self.token_a_vault_key,
                    &self.token_b_vault_key,
                    &self.pool_token_mint_key,
                    depositor_pool_key,
                    &self.token_a_mint_key,
                    &self.token_b_mint_key,
                    &self.swap_curve_key,
                    ix::DepositAllTokenTypes {
                        pool_token_amount,
                        maximum_token_a_amount,
                        maximum_token_b_amount,
                    },
                )
                .unwrap(),
                vec![
                    &mut SolanaAccount::default(),
                    &mut self.pool_account,
                    &mut self.swap_curve_account,
                    &mut SolanaAccount::default(),
                    &mut self.token_a_mint_account,
                    &mut self.token_b_mint_account,
                    &mut self.token_a_vault_account,
                    &mut self.token_b_vault_account,
                    &mut self.pool_token_mint_account,
                    depositor_token_a_account,
                    depositor_token_b_account,
                    depositor_pool_account,
                    &mut exe.clone(),
                    &mut exe.clone(),
                    &mut exe.clone(),
                ],
            )
        }

        #[allow(clippy::too_many_arguments)]
        pub fn withdraw_all_token_types(
            &mut self,
            user_key: &Pubkey,
            pool_key: &Pubkey,
            pool_account: &mut SolanaAccount,
            token_a_key: &Pubkey,
            token_a_account: &mut SolanaAccount,
            token_b_key: &Pubkey,
            token_b_account: &mut SolanaAccount,
            pool_token_amount: u64,
            minimum_token_a_amount: u64,
            minimum_token_b_amount: u64,
        ) -> ProgramResult {
            let user_transfer_authority_key = Pubkey::new_unique();
            let pool_token_program_id = pool_account.owner;
            // approve user transfer authority to take out pool tokens
            do_process_instruction(
                approve(
                    &pool_token_program_id,
                    pool_key,
                    &user_transfer_authority_key,
                    user_key,
                    &[],
                    pool_token_amount,
                )
                .unwrap(),
                vec![
                    pool_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                ],
            )
            .unwrap();

            // withdraw token a and b correctly
            let token_a_program_id = token_a_account.owner;
            let token_b_program_id = token_b_account.owner;
            do_process_instruction(
                ix::withdraw_all_token_types(
                    &crate::id(),
                    &pool_token_program_id,
                    &token_a_program_id,
                    &token_b_program_id,
                    &self.pool,
                    &self.pool_authority,
                    &user_transfer_authority_key,
                    &self.pool_token_mint_key,
                    &self.pool_token_fees_vault_key,
                    pool_key,
                    &self.token_a_vault_key,
                    &self.token_b_vault_key,
                    token_a_key,
                    token_b_key,
                    &self.token_a_mint_key,
                    &self.token_b_mint_key,
                    &self.swap_curve_key,
                    ix::WithdrawAllTokenTypes {
                        pool_token_amount,
                        minimum_token_a_amount,
                        minimum_token_b_amount,
                    },
                )
                .unwrap(),
                vec![
                    &mut self.pool_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                    &mut self.pool_token_mint_account,
                    pool_account,
                    &mut self.token_a_vault_account,
                    &mut self.token_b_vault_account,
                    token_a_account,
                    token_b_account,
                    &mut self.pool_token_fees_vault_account,
                    &mut self.token_a_mint_account,
                    &mut self.token_b_mint_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                    &mut self.swap_curve_account,
                ],
            )
        }

        #[allow(clippy::too_many_arguments)]
        pub fn deposit_single_token_type_exact_amount_in(
            &mut self,
            depositor_key: &Pubkey,
            deposit_account_key: &Pubkey,
            deposit_token_account: &mut SolanaAccount,
            deposit_pool_key: &Pubkey,
            deposit_pool_account: &mut SolanaAccount,
            source_token_amount: u64,
            minimum_pool_token_amount: u64,
        ) -> ProgramResult {
            let user_transfer_authority_key = Pubkey::new_unique();
            let source_token_program_id = deposit_token_account.owner;
            do_process_instruction(
                approve(
                    &source_token_program_id,
                    deposit_account_key,
                    &user_transfer_authority_key,
                    depositor_key,
                    &[],
                    source_token_amount,
                )
                .unwrap(),
                vec![
                    deposit_token_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                ],
            )
            .unwrap();

            let source_mint_key =
                StateWithExtensions::<Account>::unpack(&deposit_token_account.data)
                    .unwrap()
                    .base
                    .mint;
            let swap_source_key = self.get_swap_key(&source_mint_key);
            let (source_mint_key, mut source_mint_account) = self.get_token_mint(swap_source_key);

            let pool_token_program_id = deposit_pool_account.owner;
            do_process_instruction(
                ix::deposit_single_token_type_exact_amount_in(
                    &crate::id(),
                    &source_token_program_id,
                    &pool_token_program_id,
                    &self.pool,
                    &self.pool_authority,
                    &user_transfer_authority_key,
                    deposit_account_key,
                    &self.token_a_vault_key,
                    &self.token_b_vault_key,
                    &self.pool_token_mint_key,
                    deposit_pool_key,
                    &source_mint_key,
                    &self.swap_curve_key,
                    ix::DepositSingleTokenTypeExactAmountIn {
                        source_token_amount,
                        minimum_pool_token_amount,
                    },
                )
                .unwrap(),
                vec![
                    &mut self.pool_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                    deposit_token_account,
                    &mut self.token_a_vault_account,
                    &mut self.token_b_vault_account,
                    &mut self.pool_token_mint_account,
                    deposit_pool_account,
                    &mut source_mint_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                    &mut self.swap_curve_account,
                ],
            )
        }

        #[allow(clippy::too_many_arguments)]
        pub fn withdraw_single_token_type_exact_amount_out(
            &mut self,
            user_key: &Pubkey,
            pool_key: &Pubkey,
            pool_account: &mut SolanaAccount,
            destination_key: &Pubkey,
            destination_account: &mut SolanaAccount,
            destination_token_amount: u64,
            maximum_pool_token_amount: u64,
        ) -> ProgramResult {
            let user_transfer_authority_key = Pubkey::new_unique();
            let pool_token_program_id = pool_account.owner;
            // approve user transfer authority to take out pool tokens
            do_process_instruction(
                approve(
                    &pool_token_program_id,
                    pool_key,
                    &user_transfer_authority_key,
                    user_key,
                    &[],
                    maximum_pool_token_amount,
                )
                .unwrap(),
                vec![
                    pool_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                ],
            )
            .unwrap();

            let destination_mint_key =
                StateWithExtensions::<Account>::unpack(&destination_account.data)
                    .unwrap()
                    .base
                    .mint;
            let swap_destination_key = self.get_swap_key(&destination_mint_key);
            let (destination_mint_key, mut destination_mint_account) =
                self.get_token_mint(swap_destination_key);

            let destination_token_program_id = destination_account.owner;
            do_process_instruction(
                ix::withdraw_single_token_type_exact_amount_out(
                    &crate::id(),
                    &pool_token_program_id,
                    &destination_token_program_id,
                    &self.pool,
                    &self.pool_authority,
                    &user_transfer_authority_key,
                    &self.pool_token_mint_key,
                    &self.pool_token_fees_vault_key,
                    pool_key,
                    &self.token_a_vault_key,
                    &self.token_b_vault_key,
                    destination_key,
                    &destination_mint_key,
                    &self.swap_curve_key,
                    ix::WithdrawSingleTokenTypeExactAmountOut {
                        destination_token_amount,
                        maximum_pool_token_amount,
                    },
                )
                .unwrap(),
                vec![
                    &mut self.pool_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                    &mut self.pool_token_mint_account,
                    pool_account,
                    &mut self.token_a_vault_account,
                    &mut self.token_b_vault_account,
                    destination_account,
                    &mut self.pool_token_fees_vault_account,
                    &mut destination_mint_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                    &mut self.swap_curve_account,
                ],
            )
        }
    }

    fn mint_minimum_balance() -> u64 {
        Rent::default().minimum_balance(spl_token::state::Mint::get_packed_len())
    }

    fn account_minimum_balance() -> u64 {
        Rent::default().minimum_balance(spl_token::state::Account::get_packed_len())
    }

    fn do_process_instruction_with_fee_constraints(
        instruction: Instruction,
        accounts: Vec<&mut SolanaAccount>,
        _swap_constraints: &Option<SwapConstraints>, // todo - elliot - compile time constraints
    ) -> ProgramResult {
        test_syscall_stubs();

        // approximate the logic in the actual runtime which runs the instruction
        // and only updates accounts if the instruction is successful
        let mut account_clones = accounts.iter().map(|x| (*x).clone()).collect::<Vec<_>>();
        let mut account_infos = instruction
            .accounts
            .iter()
            .zip(account_clones.iter_mut())
            .map(|(account_meta, account)| {
                AccountInfo::new(
                    &account_meta.pubkey,
                    account_meta.is_signer,
                    account_meta.is_writable,
                    &mut account.lamports,
                    &mut account.data,
                    &account.owner,
                    account.executable,
                    account.rent_epoch,
                )
            })
            .collect::<Vec<_>>();

        let res = if instruction.program_id == crate::id() {
            crate::entry(&instruction.program_id, &account_infos, &instruction.data)
        } else if instruction.program_id == spl_token::id() {
            spl_token::processor::Processor::process(
                &instruction.program_id,
                &account_infos,
                &instruction.data,
            )
        } else if instruction.program_id == spl_token_2022::id() {
            spl_token_2022::processor::Processor::process(
                &instruction.program_id,
                &account_infos,
                &instruction.data,
            )
        } else {
            Err(ProgramError::IncorrectProgramId)
        };

        if res.is_ok() {
            let mut account_metas = instruction
                .accounts
                .iter()
                .zip(accounts)
                .map(|(account_meta, account)| (&account_meta.pubkey, account))
                .collect::<Vec<_>>();
            for account_info in account_infos.iter_mut() {
                for account_meta in account_metas.iter_mut() {
                    if account_info.key == account_meta.0 {
                        let account = &mut account_meta.1;
                        account.owner = *account_info.owner;
                        account.lamports = **account_info.lamports.borrow();
                        account.data = account_info.data.borrow().to_vec();
                    }
                }
            }
        }
        res
    }

    fn do_process_instruction(
        instruction: Instruction,
        accounts: Vec<&mut SolanaAccount>,
    ) -> ProgramResult {
        do_process_instruction_with_fee_constraints(instruction, accounts, &SWAP_CONSTRAINTS)
    }

    fn create_token_account(
        program_id: &Pubkey,
        mint_key: &Pubkey,
        mint_account: &mut SolanaAccount,
        mint_authority_key: &Pubkey,
        account_owner_key: &Pubkey,
        amount: u64,
    ) -> (Pubkey, SolanaAccount) {
        let account_key = Pubkey::new_unique();

        (
            account_key,
            create_token_account_with_address(
                &account_key,
                program_id,
                mint_key,
                mint_account,
                mint_authority_key,
                account_owner_key,
                amount,
            ),
        )
    }

    fn create_token_account_with_address(
        account_key: &Pubkey,
        program_id: &Pubkey,
        mint_key: &Pubkey,
        mint_account: &mut SolanaAccount,
        mint_authority_key: &Pubkey,
        account_owner_key: &Pubkey,
        amount: u64,
    ) -> SolanaAccount {
        let space = if *program_id == spl_token_2022::id() {
            ExtensionType::get_account_len::<Account>(&[
                ExtensionType::ImmutableOwner,
                ExtensionType::TransferFeeAmount,
            ])
        } else {
            Account::get_packed_len()
        };
        let minimum_balance = Rent::default().minimum_balance(space);
        let mut account_account = SolanaAccount::new(minimum_balance, space, program_id);
        let mut mint_authority_account = SolanaAccount::default();
        let mut rent_sysvar_account = create_account_for_test(&Rent::free());

        // no-ops in normal token, so we're good to run it either way
        do_process_instruction(
            initialize_immutable_owner(program_id, account_key).unwrap(),
            vec![&mut account_account],
        )
        .unwrap();

        do_process_instruction(
            initialize_account(program_id, account_key, mint_key, account_owner_key).unwrap(),
            vec![
                &mut account_account,
                mint_account,
                &mut mint_authority_account,
                &mut rent_sysvar_account,
            ],
        )
        .unwrap();

        if amount > 0 {
            do_process_instruction(
                mint_to(
                    program_id,
                    mint_key,
                    account_key,
                    mint_authority_key,
                    &[],
                    amount,
                )
                .unwrap(),
                vec![
                    mint_account,
                    &mut account_account,
                    &mut mint_authority_account,
                ],
            )
            .unwrap();
        }

        account_account
    }

    fn create_mint(
        program_id: &Pubkey,
        authority_key: &Pubkey,
        freeze_authority: Option<&Pubkey>,
        close_authority: Option<&Pubkey>,
        fees: &TransferFee,
    ) -> (Pubkey, SolanaAccount) {
        let mint_key = Pubkey::new_unique();

        (
            mint_key,
            create_mint_with_address(
                &mint_key,
                program_id,
                authority_key,
                freeze_authority,
                close_authority,
                6,
                fees,
            ),
        )
    }

    fn create_mint_with_address(
        mint_key: &Pubkey,
        program_id: &Pubkey,
        authority_key: &Pubkey,
        freeze_authority: Option<&Pubkey>,
        close_authority: Option<&Pubkey>,
        decimals: u8,
        fees: &TransferFee,
    ) -> SolanaAccount {
        let space = if *program_id == spl_token_2022::id() {
            if close_authority.is_some() {
                ExtensionType::get_account_len::<Mint>(&[
                    ExtensionType::MintCloseAuthority,
                    ExtensionType::TransferFeeConfig,
                ])
            } else {
                ExtensionType::get_account_len::<Mint>(&[ExtensionType::TransferFeeConfig])
            }
        } else {
            Mint::get_packed_len()
        };
        let minimum_balance = Rent::default().minimum_balance(space);
        let mut mint_account = SolanaAccount::new(minimum_balance, space, program_id);
        let mut rent_sysvar_account = create_account_for_test(&Rent::free());

        if *program_id == spl_token_2022::id() {
            if close_authority.is_some() {
                do_process_instruction(
                    initialize_mint_close_authority(program_id, mint_key, close_authority).unwrap(),
                    vec![&mut mint_account],
                )
                .unwrap();
            }
            do_process_instruction(
                initialize_transfer_fee_config(
                    program_id,
                    mint_key,
                    freeze_authority,
                    freeze_authority,
                    fees.transfer_fee_basis_points.into(),
                    fees.maximum_fee.into(),
                )
                .unwrap(),
                vec![&mut mint_account],
            )
            .unwrap();
        }
        do_process_instruction(
            initialize_mint(
                program_id,
                mint_key,
                authority_key,
                freeze_authority,
                decimals,
            )
            .unwrap(),
            vec![&mut mint_account, &mut rent_sysvar_account],
        )
        .unwrap();

        mint_account
    }

    fn get_token_account_space(token_program: &Pubkey, mint: &SolanaAccount) -> usize {
        if token_program == &spl_token_2022::id() {
            // calculate the space for the token account with required extensions
            let mint = StateWithExtensions::<Mint>::unpack(&mint.data).unwrap();
            let mint_extensions: Vec<ExtensionType> =
                BaseStateWithExtensions::get_extension_types(&mint).unwrap();

            let required_extensions =
                ExtensionType::get_required_init_account_extensions(&mint_extensions);

            ExtensionType::get_account_len::<Account>(&required_extensions)
        } else {
            anchor_spl::token::TokenAccount::LEN
        }
    }

    #[test_case(spl_token::id(); "token")]
    #[test_case(spl_token_2022::id(); "token-2022")]
    fn test_token_program_id_error(token_program_id: Pubkey) {
        test_syscall_stubs();
        let pool_key = Pubkey::new_unique();
        let mut mint = (Pubkey::new_unique(), SolanaAccount::default());
        let mut destination = (Pubkey::new_unique(), SolanaAccount::default());
        let token_program = (token_program_id, SolanaAccount::default());
        let (pool_authority_key, pool_authority_bump_seed) = Pubkey::find_program_address(
            &[b"pauthority".as_ref(), pool_key.as_ref()],
            &crate::id(),
        );
        let mut authority = (pool_authority_key, SolanaAccount::default());
        let swap_bytes = pool_key.to_bytes();
        let authority_signature_seeds = [
            b"pauthority".as_ref(),
            &swap_bytes[..32],
            &[pool_authority_bump_seed],
        ];
        let signers = &[&authority_signature_seeds[..]];
        let ix = mint_to(
            &token_program.0,
            &mint.0,
            &destination.0,
            &authority.0,
            &[],
            10,
        )
        .unwrap();
        let mint = (&mut mint).into();
        let destination = (&mut destination).into();
        let authority = (&mut authority).into();

        let err = invoke_signed(&ix, &[mint, destination, authority], signers).unwrap_err();
        assert_eq!(err, ProgramError::InvalidAccountData);
    }

    #[test_case(spl_token::id(); "token")]
    #[test_case(spl_token_2022::id(); "token-2022")]
    fn test_token_error(token_program_id: Pubkey) {
        test_syscall_stubs();
        let pool_key = Pubkey::new_unique();
        let mut mint = (
            Pubkey::new_unique(),
            SolanaAccount::new(
                mint_minimum_balance(),
                spl_token::state::Mint::get_packed_len(),
                &token_program_id,
            ),
        );
        let mut destination = (
            Pubkey::new_unique(),
            SolanaAccount::new(
                account_minimum_balance(),
                spl_token::state::Account::get_packed_len(),
                &token_program_id,
            ),
        );
        let mut token_program = (token_program_id, SolanaAccount::default());
        let (pool_authority_key, pool_authority_bump_seed) =
            Pubkey::find_program_address(&[&pool_key.to_bytes()[..]], &crate::id());
        let mut authority = (pool_authority_key, SolanaAccount::default());
        let swap_bytes = pool_key.to_bytes();
        let authority_signature_seeds = [&swap_bytes[..32], &[pool_authority_bump_seed]];
        let signers = &[&authority_signature_seeds[..]];
        let mut rent_sysvar = (
            Pubkey::new_unique(),
            create_account_for_test(&Rent::default()),
        );
        do_process_instruction(
            initialize_mint(
                &token_program.0,
                &mint.0,
                &authority.0,
                Some(&authority.0),
                2,
            )
            .unwrap(),
            vec![&mut mint.1, &mut rent_sysvar.1],
        )
        .unwrap();
        do_process_instruction(
            initialize_account(&token_program.0, &destination.0, &mint.0, &authority.0).unwrap(),
            vec![
                &mut destination.1,
                &mut mint.1,
                &mut authority.1,
                &mut rent_sysvar.1,
                &mut token_program.1,
            ],
        )
        .unwrap();
        do_process_instruction(
            freeze_account(&token_program.0, &destination.0, &mint.0, &authority.0, &[]).unwrap(),
            vec![
                &mut destination.1,
                &mut mint.1,
                &mut authority.1,
                &mut token_program.1,
            ],
        )
        .unwrap();
        let ix = mint_to(
            &token_program.0,
            &mint.0,
            &destination.0,
            &authority.0,
            &[],
            10,
        )
        .unwrap();
        let mint_info = (&mut mint).into();
        let destination_info = (&mut destination).into();
        let authority_info = (&mut authority).into();
        let token_program_info = (&mut token_program).into();

        let err = invoke_signed_wrapper::<TokenError>(
            &ix,
            &[
                mint_info,
                destination_info,
                authority_info,
                token_program_info,
            ],
            signers,
        )
        .unwrap_err();
        assert_eq!(err, ProgramError::Custom(TokenError::AccountFrozen as u32));
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_initialize(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 2;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 10;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 5;
        let host_fee_numerator = 20;
        let host_fee_denominator = 100;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 1000;
        let token_b_amount = 2000;
        let pool_token_amount = 10;
        let curve_params = CurveParameters::ConstantProduct;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        // uninitialized token a account
        {
            let old_account = accounts.token_a_vault_account;
            accounts.token_a_vault_account = SolanaAccount::new(0, 0, &token_a_program_id);
            assert_eq!(
                Err(ProgramError::InvalidAccountData),
                accounts.initialize_pool()
            );
            accounts.token_a_vault_account = old_account;
        }

        // uninitialized token b account
        {
            let old_account = accounts.token_b_vault_account;
            accounts.token_b_vault_account = SolanaAccount::new(0, 0, &token_b_program_id);
            assert_eq!(
                Err(ProgramError::InvalidAccountData),
                accounts.initialize_pool()
            );
            accounts.token_b_vault_account = old_account;
        }

        // initialized pool mint
        {
            let old_account = accounts.pool_token_mint_account;
            accounts.pool_token_mint_account = create_mint_with_address(
                &accounts.pool_token_mint_key,
                old_account.owner(),
                &Pubkey::new_unique(),
                None,
                None,
                6,
                &TransferFee::default(),
            );
            assert_eq!(
                Err(spl_token::error::TokenError::AlreadyInUse.into()),
                accounts.initialize_pool()
            );
            accounts.pool_token_mint_account = old_account;
        }

        // token A account owner is not admin authority
        {
            let fake_admin = Pubkey::new_unique();
            let (_token_a_ata_key, admin_authority_token_a_ata_account) = create_token_account(
                &token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &fake_admin,
                1000,
            );
            let old_account = accounts.admin_authority_token_a_ata_account;
            accounts.admin_authority_token_a_ata_account = admin_authority_token_a_ata_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::ConstraintTokenOwner.into()
                )),
                accounts.initialize_pool()
            );
            accounts.admin_authority_token_a_ata_account = old_account;
        }

        // token B account owner is not admin authority
        {
            let fake_admin = Pubkey::new_unique();
            let (_token_b_key, token_b_account) = create_token_account(
                &token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &fake_admin,
                0,
            );
            let old_account = accounts.admin_authority_token_b_ata_account;
            accounts.admin_authority_token_b_ata_account = token_b_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::ConstraintTokenOwner.into()
                )),
                accounts.initialize_pool()
            );
            accounts.admin_authority_token_b_ata_account = old_account;
        }

        // pool mint already initialized with freeze authority
        {
            let (_pool_mint_key, pool_mint_account) = create_mint(
                &accounts.pool_token_program_id,
                &accounts.pool_authority,
                Some(&user_key),
                None,
                &TransferFee::default(),
            );
            let old_mint = accounts.pool_token_mint_account;
            accounts.pool_token_mint_account = pool_mint_account;
            assert_eq!(
                Err(TokenError::AlreadyInUse.into()),
                accounts.initialize_pool()
            );
            accounts.pool_token_mint_account = old_mint;
        }

        // token A account owned by wrong program
        {
            let (_token_a_key, mut token_a_account) = create_token_account(
                &token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &user_key,
                token_a_amount,
            );
            token_a_account.owner = crate::id();
            let old_account = accounts.admin_authority_token_a_ata_account;
            accounts.admin_authority_token_a_ata_account = token_a_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountOwnedByWrongProgram.into()
                )),
                accounts.initialize_pool()
            );
            accounts.admin_authority_token_a_ata_account = old_account;
        }

        // token B account owned by wrong program
        {
            let (_token_b_key, mut token_b_account) = create_token_account(
                &token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &user_key,
                token_b_amount,
            );
            token_b_account.owner = crate::id();
            let old_account = accounts.admin_authority_token_b_ata_account;
            accounts.admin_authority_token_b_ata_account = token_b_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountOwnedByWrongProgram.into()
                )),
                accounts.initialize_pool()
            );
            accounts.admin_authority_token_b_ata_account = old_account;
        }

        // empty token A account
        {
            let (_token_a_key, token_a_account) = create_token_account(
                &token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &user_key,
                0,
            );
            let old_account = accounts.admin_authority_token_a_ata_account;
            accounts.admin_authority_token_a_ata_account = token_a_account;
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.initialize_pool()
            );
            accounts.admin_authority_token_a_ata_account = old_account;
        }

        // empty token B account
        {
            let (_token_b_key, token_b_account) = create_token_account(
                &token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &user_key,
                0,
            );
            let old_account = accounts.admin_authority_token_b_ata_account;
            accounts.admin_authority_token_b_ata_account = token_b_account;
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.initialize_pool()
            );
            accounts.admin_authority_token_b_ata_account = old_account;
        }

        // 0 token A initial supply
        {
            let old_initial_supply = accounts.initial_supply;
            accounts.initial_supply = InitialSupply {
                initial_supply_a: 0,
                ..old_initial_supply
            };
            assert_eq!(
                Err(ProgramError::Custom(SwapError::EmptySupply.into())),
                accounts.initialize_pool()
            );
            accounts.initial_supply = old_initial_supply;
        }

        // 0 token B initial supply
        {
            let old_initial_supply = accounts.initial_supply;
            accounts.initial_supply = InitialSupply {
                initial_supply_b: 0,
                ..old_initial_supply
            };
            assert_eq!(
                Err(ProgramError::Custom(SwapError::EmptySupply.into())),
                accounts.initialize_pool()
            );
            accounts.initial_supply = old_initial_supply;
        }

        // invalid pool tokens
        {
            let old_mint = accounts.pool_token_mint_account;
            let old_pool_account = accounts.admin_authority_pool_token_ata_account;

            let (_pool_mint_key, pool_mint_account) = create_mint(
                &accounts.pool_token_program_id,
                &accounts.pool_authority,
                None,
                None,
                &TransferFee::default(),
            );
            accounts.pool_token_mint_account = pool_mint_account;

            let (_empty_pool_token_key, empty_pool_token_account) = create_token_account(
                &accounts.pool_token_program_id,
                &accounts.pool_token_mint_key,
                &mut accounts.pool_token_mint_account,
                &accounts.pool_authority,
                &user_key,
                0,
            );

            let (_pool_token_key, pool_token_account) = create_token_account(
                &accounts.pool_token_program_id,
                &accounts.pool_token_mint_key,
                &mut accounts.pool_token_mint_account,
                &accounts.pool_authority,
                &user_key,
                pool_token_amount,
            );

            // non-empty pool token account
            accounts.admin_authority_pool_token_ata_account = pool_token_account;
            assert_eq!(
                Err(TokenError::AlreadyInUse.into()),
                accounts.initialize_pool()
            );

            // pool tokens already in circulation
            accounts.admin_authority_pool_token_ata_account = empty_pool_token_account;
            assert_eq!(
                Err(TokenError::AlreadyInUse.into()),
                accounts.initialize_pool()
            );

            accounts.pool_token_mint_account = old_mint;
            accounts.admin_authority_pool_token_ata_account = old_pool_account;
        }

        // pool fee account already initialized
        {
            // wrong mint
            let (pool_mint_key, mut pool_mint_account) = create_mint(
                &pool_token_program_id,
                &accounts.pool_authority,
                None,
                None,
                &TransferFee::default(),
            );
            let (_pool_fee_key, pool_fee_account) = create_token_account(
                &pool_token_program_id,
                &pool_mint_key,
                &mut pool_mint_account,
                &user_key,
                &user_key,
                0,
            );
            let old_account = accounts.pool_token_fees_vault_account;
            accounts.pool_token_fees_vault_account = pool_fee_account;
            assert_eq!(
                Err(TokenError::AlreadyInUse.into()),
                accounts.initialize_pool()
            );
            accounts.pool_token_fees_vault_account = old_account;
        }

        // wrong pool token program id
        {
            let wrong_pool_token_program_id = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);
            assert_eq!(
                Err(ProgramError::Custom(AnchorError::InvalidProgramId.into())),
                do_process_instruction(
                    ix::initialize_pool(
                        &crate::id(),
                        &accounts.admin_authority,
                        &accounts.pool,
                        &accounts.swap_curve_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &accounts.pool_authority,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &accounts.admin_authority_token_a_ata_key,
                        &accounts.admin_authority_token_b_ata_key,
                        &accounts.admin_authority_pool_token_ata_key,
                        &wrong_pool_token_program_id,
                        &accounts.token_a_program_id,
                        &accounts.token_b_program_id,
                        accounts.fees,
                        accounts.initial_supply.clone(),
                        accounts.curve_params.clone(),
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut accounts.admin_authority_token_a_ata_account,
                        &mut accounts.admin_authority_token_b_ata_account,
                        &mut accounts.admin_authority_pool_token_ata_account,
                        &mut exe.clone(), // system_program
                        &mut create_account_for_test(&Rent::default()),
                        &mut exe.clone(), // pool_token_program
                        &mut exe.clone(), // token_a_program
                        &mut exe.clone(), // token_b_program
                    ],
                )
            );
        }

        // create swap with same token A and B
        {
            let (token_b_vault_key, _token_b_vault_bump_seed) = Pubkey::find_program_address(
                &[
                    b"pvault_b".as_ref(),
                    accounts.pool.as_ref(),
                    accounts.token_a_mint_key.as_ref(),
                ],
                &crate::id(),
            );
            let token_b_vault_account = SolanaAccount::new(
                u32::MAX as u64,
                get_token_account_space(
                    &accounts.token_a_program_id,
                    &accounts.token_a_mint_account,
                ), // todo size needed because syscall not stubbed
                &accounts.token_a_program_id, // todo - this should be system but we no-op the system program calls
            );
            let (admin_authority_token_b_ata_key, admin_authority_token_b_ata_account) =
                create_token_account(
                    &token_a_program_id,
                    &accounts.token_a_mint_key,
                    &mut accounts.token_a_mint_account,
                    &user_key,
                    &user_key,
                    token_b_amount,
                );
            let old_mint_key = accounts.token_b_mint_key;
            let old_mint = accounts.token_b_mint_account;
            let old_account_key = accounts.token_b_vault_key;
            let old_account = accounts.token_b_vault_account;
            let old_ata_key = accounts.admin_authority_token_b_ata_key;
            let old_token_program = accounts.token_b_program_id;
            let old_ata_account = accounts.admin_authority_token_b_ata_account;
            accounts.token_b_mint_key = accounts.token_a_mint_key;
            accounts.token_b_mint_account = accounts.token_a_mint_account.clone();
            accounts.token_b_vault_key = token_b_vault_key;
            accounts.token_b_vault_account = token_b_vault_account;
            accounts.admin_authority_token_b_ata_key = admin_authority_token_b_ata_key;
            accounts.admin_authority_token_b_ata_account = admin_authority_token_b_ata_account;
            accounts.token_b_program_id = token_a_program_id;
            assert_eq!(
                Err(ProgramError::Custom(SwapError::RepeatedMint.into())),
                accounts.initialize_pool()
            );
            accounts.token_b_mint_key = old_mint_key;
            accounts.token_b_mint_account = old_mint;
            accounts.token_b_vault_key = old_account_key;
            accounts.token_b_vault_account = old_account;
            accounts.admin_authority_token_b_ata_key = old_ata_key;
            accounts.admin_authority_token_b_ata_account = old_ata_account;
            accounts.token_b_program_id = old_token_program;
        }

        // create valid swap
        accounts.initialize_pool().unwrap();

        // create invalid flat swap
        {
            let token_b_price = 0;
            let fees = Fees {
                trade_fee_numerator,
                trade_fee_denominator,
                owner_trade_fee_numerator,
                owner_trade_fee_denominator,
                owner_withdraw_fee_numerator,
                owner_withdraw_fee_denominator,
                host_fee_numerator,
                host_fee_denominator,
            };
            let curve_params = CurveParameters::ConstantPrice { token_b_price };
            let mut accounts = SwapAccountInfo::new(
                &user_key,
                fees,
                SwapTransferFees::default(),
                curve_params,
                InitialSupply {
                    initial_supply_a: token_a_amount,
                    initial_supply_b: token_b_amount,
                },
                &pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
            );
            assert_eq!(
                Err(ProgramError::Custom(SwapError::InvalidCurve.into())),
                accounts.initialize_pool()
            );
        }

        // create valid flat swap
        {
            let fees = Fees {
                trade_fee_numerator,
                trade_fee_denominator,
                owner_trade_fee_numerator,
                owner_trade_fee_denominator,
                owner_withdraw_fee_numerator,
                owner_withdraw_fee_denominator,
                host_fee_numerator,
                host_fee_denominator,
            };
            let token_b_price = 10_000;
            let curve_params = CurveParameters::ConstantPrice { token_b_price };

            let mut accounts = SwapAccountInfo::new(
                &user_key,
                fees,
                SwapTransferFees::default(),
                curve_params,
                InitialSupply {
                    initial_supply_a: token_a_amount,
                    initial_supply_b: token_b_amount,
                },
                &pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
            );
            accounts.initialize_pool().unwrap();
        }

        // create invalid offset swap
        {
            let token_b_offset = 0;
            let fees = Fees {
                trade_fee_numerator,
                trade_fee_denominator,
                owner_trade_fee_numerator,
                owner_trade_fee_denominator,
                owner_withdraw_fee_numerator,
                owner_withdraw_fee_denominator,
                host_fee_numerator,
                host_fee_denominator,
            };
            let curve_params = CurveParameters::Offset { token_b_offset };
            let mut accounts = SwapAccountInfo::new(
                &user_key,
                fees,
                SwapTransferFees::default(),
                curve_params,
                InitialSupply {
                    initial_supply_a: token_a_amount,
                    initial_supply_b: token_b_amount,
                },
                &pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
            );
            assert_eq!(
                Err(ProgramError::Custom(SwapError::InvalidCurve.into())),
                accounts.initialize_pool()
            );
        }

        // create valid offset swap
        {
            let token_b_offset = 10;
            let fees = Fees {
                trade_fee_numerator,
                trade_fee_denominator,
                owner_trade_fee_numerator,
                owner_trade_fee_denominator,
                owner_withdraw_fee_numerator,
                owner_withdraw_fee_denominator,
                host_fee_numerator,
                host_fee_denominator,
            };

            let curve_params = CurveParameters::Offset { token_b_offset };
            let mut accounts = SwapAccountInfo::new(
                &user_key,
                fees,
                SwapTransferFees::default(),
                curve_params,
                InitialSupply {
                    initial_supply_a: token_a_amount,
                    initial_supply_b: token_b_amount,
                },
                &pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
            );
            accounts.initialize_pool().unwrap();
        }

        // todo - elliot - compile-time constraints
        // // wrong owner key in constraint
        // {
        //     let new_key = Pubkey::new_unique();
        //     let trade_fee_numerator = 25;
        //     let trade_fee_denominator = 10000;
        //     let owner_trade_fee_numerator = 5;
        //     let owner_trade_fee_denominator = 10000;
        //     let host_fee_numerator = 20;
        //     let host_fee_denominator = 100;
        //     let fees = Fees {
        //         trade_fee_numerator,
        //         trade_fee_denominator,
        //         owner_trade_fee_numerator,
        //         owner_trade_fee_denominator,
        //         owner_withdraw_fee_numerator,
        //         owner_withdraw_fee_denominator,
        //         host_fee_numerator,
        //         host_fee_denominator,
        //     };
        //     let curve_params = CurveParameters::ConstantProduct;
        //     let owner_key = &new_key.to_string();
        //     let valid_curve_types = &[CurveType::ConstantProduct];
        //     let constraints = Some(SwapConstraints {
        //         owner_key,
        //         valid_curve_types,
        //         fees: &fees,
        //     });
        //     let mut accounts = SwapAccountInfo::new(
        //         &user_key,
        //         fees.clone(),
        //         SwapTransferFees::default(),
        //         curve_params.clone(),
        //         token_a_amount,
        //         token_b_amount,
        //         &token_a_program_id,
        //         &token_b_program_id,
        //     );
        //     let exe = &mut SolanaAccount::default();
        //     exe.set_executable(true);
        //     assert_eq!(
        //         Err(SwapError::InvalidOwner.into()),
        //         do_process_instruction_with_fee_constraints(
        //             ix::initialize_pool(
        //                 &crate::id(),
        //                 &accounts.admin_authority,
        //                 &accounts.pool,
        //                 &accounts.swap_curve_key,
        //                 &accounts.token_a_mint_key,
        //                 &accounts.token_b_mint_key,
        //                 &accounts.token_a_vault_key,
        //                 &accounts.token_b_vault_key,
        //                 &accounts.pool_authority,
        //                 &accounts.pool_token_mint_key,
        //                 &accounts.pool_token_fees_vault_key,
        //                 &accounts.admin_authority_pool_token_ata_key,
        //                 accounts.fees.clone(),
        //                 accounts.curve_params.clone(),
        //             )
        //             .unwrap(),
        //             vec![
        //                 &mut SolanaAccount::default(),
        //                 &mut accounts.pool_account,
        //                 &mut accounts.swap_curve_account,
        //                 &mut SolanaAccount::default(),
        //                 &mut accounts.token_a_mint_account,
        //                 &mut accounts.token_b_mint_account,
        //                 &mut accounts.token_a_vault_account,
        //                 &mut accounts.token_b_vault_account,
        //                 &mut accounts.pool_token_mint_account,
        //                 &mut accounts.pool_token_fees_vault_account,
        //                 &mut accounts.admin_authority_pool_token_ata_account,
        //                 &mut exe.clone(),
        //                 &mut create_account_for_test(&Rent::default()),
        //                 &mut exe.clone(),
        //             ],
        //             &constraints,
        //         )
        //     );
        // }
        //
        // // wrong fee in constraint
        // {
        //     let trade_fee_numerator = 25;
        //     let trade_fee_denominator = 10000;
        //     let owner_trade_fee_numerator = 5;
        //     let owner_trade_fee_denominator = 10000;
        //     let host_fee_numerator = 20;
        //     let host_fee_denominator = 100;
        //     let fees = Fees {
        //         trade_fee_numerator,
        //         trade_fee_denominator,
        //         owner_trade_fee_numerator,
        //         owner_trade_fee_denominator,
        //         owner_withdraw_fee_numerator,
        //         owner_withdraw_fee_denominator,
        //         host_fee_numerator,
        //         host_fee_denominator,
        //     };
        //
        //     let curve_params = CurveParameters::ConstantProduct;
        //     let owner_key = &user_key.to_string();
        //     let valid_curve_types = &[CurveType::ConstantProduct];
        //     let constraints = Some(SwapConstraints {
        //         owner_key,
        //         valid_curve_types,
        //         fees: &fees,
        //     });
        //     let mut bad_fees = fees.clone();
        //     bad_fees.trade_fee_numerator = trade_fee_numerator - 1;
        //     let mut accounts = SwapAccountInfo::new(
        //         &user_key,
        //         bad_fees,
        //         SwapTransferFees::default(),
        //         curve_params,
        //         token_a_amount,
        //         token_b_amount,
        //         &token_a_program_id,
        //         &token_b_program_id,
        //     );
        //     let exe = &mut SolanaAccount::default();
        //     exe.set_executable(true);
        //     assert_eq!(
        //         Err(SwapError::InvalidFee.into()),
        //         do_process_instruction_with_fee_constraints(
        //             ix::initialize_pool(
        //                 &crate::id(),
        //                 &accounts.admin_authority,
        //                 &accounts.pool,
        //                 &accounts.swap_curve_key,
        //                 &accounts.token_a_mint_key,
        //                 &accounts.token_b_mint_key,
        //                 &accounts.token_a_vault_key,
        //                 &accounts.token_b_vault_key,
        //                 &accounts.pool_authority,
        //                 &accounts.pool_token_mint_key,
        //                 &accounts.pool_token_fees_vault_key,
        //                 &accounts.admin_authority_pool_token_ata_key,
        //                 accounts.fees.clone(),
        //                 accounts.curve_params.clone(),
        //             )
        //             .unwrap(),
        //             vec![
        //                 &mut SolanaAccount::default(),
        //                 &mut accounts.pool_account,
        //                 &mut accounts.swap_curve_account,
        //                 &mut SolanaAccount::default(),
        //                 &mut accounts.token_a_mint_account,
        //                 &mut accounts.token_b_mint_account,
        //                 &mut accounts.token_a_vault_account,
        //                 &mut accounts.token_b_vault_account,
        //                 &mut accounts.pool_token_mint_account,
        //                 &mut accounts.pool_token_fees_vault_account,
        //                 &mut accounts.admin_authority_pool_token_ata_account,
        //                 &mut exe.clone(),
        //                 &mut create_account_for_test(&Rent::default()),
        //                 &mut exe.clone(),
        //             ],
        //             &constraints,
        //         )
        //     );
        // }

        // create valid swap with constraints
        {
            let trade_fee_numerator = 25;
            let trade_fee_denominator = 10000;
            let owner_trade_fee_numerator = 5;
            let owner_trade_fee_denominator = 10000;
            let host_fee_numerator = 20;
            let host_fee_denominator = 100;
            let fees = Fees {
                trade_fee_numerator,
                trade_fee_denominator,
                owner_trade_fee_numerator,
                owner_trade_fee_denominator,
                owner_withdraw_fee_numerator,
                owner_withdraw_fee_denominator,
                host_fee_numerator,
                host_fee_denominator,
            };
            let curve_params = CurveParameters::ConstantProduct;
            let owner_key = &user_key.to_string();
            let valid_curve_types = &[CurveType::ConstantProduct];
            let constraints = Some(SwapConstraints {
                owner_key,
                valid_curve_types,
                fees: &fees,
            });
            let mut accounts = SwapAccountInfo::new(
                &user_key,
                fees,
                SwapTransferFees::default(),
                curve_params,
                InitialSupply {
                    initial_supply_a: token_a_amount,
                    initial_supply_b: token_b_amount,
                },
                &pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
            );
            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);
            do_process_instruction_with_fee_constraints(
                ix::initialize_pool(
                    &crate::id(),
                    &accounts.admin_authority,
                    &accounts.pool,
                    &accounts.swap_curve_key,
                    &accounts.token_a_mint_key,
                    &accounts.token_b_mint_key,
                    &accounts.token_a_vault_key,
                    &accounts.token_b_vault_key,
                    &accounts.pool_authority,
                    &accounts.pool_token_mint_key,
                    &accounts.pool_token_fees_vault_key,
                    &accounts.admin_authority_token_a_ata_key,
                    &accounts.admin_authority_token_b_ata_key,
                    &accounts.admin_authority_pool_token_ata_key,
                    &accounts.pool_token_program_id,
                    &accounts.token_a_program_id,
                    &accounts.token_b_program_id,
                    accounts.fees,
                    accounts.initial_supply,
                    accounts.curve_params.clone(),
                )
                .unwrap(),
                vec![
                    &mut SolanaAccount::default(),
                    &mut accounts.pool_account,
                    &mut accounts.swap_curve_account,
                    &mut SolanaAccount::default(),
                    &mut accounts.token_a_mint_account,
                    &mut accounts.token_b_mint_account,
                    &mut accounts.token_a_vault_account,
                    &mut accounts.token_b_vault_account,
                    &mut accounts.pool_token_mint_account,
                    &mut accounts.pool_token_fees_vault_account,
                    &mut accounts.admin_authority_token_a_ata_account,
                    &mut accounts.admin_authority_token_b_ata_account,
                    &mut accounts.admin_authority_pool_token_ata_account,
                    &mut exe.clone(), // system_program
                    &mut create_account_for_test(&Rent::default()),
                    &mut exe.clone(), // pool_token_program
                    &mut exe.clone(), // token_a_program
                    &mut exe.clone(), // token_b_program
                ],
                &constraints,
            )
            .unwrap();
        }

        // create again
        {
            assert_eq!(
                Err(TokenError::AlreadyInUse.into()),
                accounts.initialize_pool()
            );
        }

        let mut data = accounts.pool_account.data.as_ref();
        let swap_pool: SwapPool = AccountDeserialize::try_deserialize(&mut data).unwrap();
        assert!(swap_pool.is_initialized());
        assert_eq!(swap_pool.bump_seed(), accounts.pool_authority_bump_seed);
        assert_eq!(swap_pool.pool_authority, accounts.pool_authority);
        assert_eq!(swap_pool.token_program_id, accounts.pool_token_program_id);
        assert_eq!(swap_pool.curve_type(), accounts.swap_curve.curve_type);
        assert_eq!(swap_pool.swap_curve, accounts.swap_curve_key);
        assert_eq!(*swap_pool.token_a_account(), accounts.token_a_vault_key);
        assert_eq!(*swap_pool.token_b_account(), accounts.token_b_vault_key);
        assert_eq!(*swap_pool.pool_mint(), accounts.pool_token_mint_key);
        assert_eq!(*swap_pool.token_a_mint(), accounts.token_a_mint_key);
        assert_eq!(*swap_pool.token_b_mint(), accounts.token_b_mint_key);
        assert_eq!(
            *swap_pool.pool_fee_account(),
            accounts.pool_token_fees_vault_key
        );
        assert_eq!(swap_pool.fees, accounts.fees);
        let token_a =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data).unwrap();
        assert_eq!(token_a.base.amount, token_a_amount);
        let token_b =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data).unwrap();
        assert_eq!(token_b.base.amount, token_b_amount);
        let pool_account = StateWithExtensions::<Account>::unpack(
            &accounts.admin_authority_pool_token_ata_account.data,
        )
        .unwrap();
        let pool_mint =
            StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data).unwrap();
        assert_eq!(pool_mint.base.supply, pool_account.base.amount);
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_deposit(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();
        let depositor_key = Pubkey::new_unique();
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 2;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 10;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 5;
        let host_fee_numerator = 20;
        let host_fee_denominator = 100;

        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 1000;
        let token_b_amount = 9000;
        let curve_params = CurveParameters::ConstantProduct;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        // depositing 10% of the current pool amount in token A and B means
        // that our pool tokens will be worth 1 / 10 of the current pool amount
        let pool_amount = INITIAL_SWAP_POOL_AMOUNT / 10;
        let deposit_a = token_a_amount / 10;
        let deposit_b = token_b_amount / 10;

        // swap not initialized
        {
            let (token_a_key, mut token_a_account) = create_token_account(
                &accounts.token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &depositor_key,
                deposit_a,
            );
            let (token_b_key, mut token_b_account) = create_token_account(
                &accounts.token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &depositor_key,
                deposit_b,
            );
            // use token A mint because pool mint not initialized
            let (pool_key, mut pool_account) = create_token_account(
                &accounts.token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &depositor_key,
                0,
            );
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountDiscriminatorMismatch.into()
                )),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
        }

        accounts.initialize_pool().unwrap();

        // wrong owner for pool account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let old_pool_account = accounts.pool_account;
            let mut wrong_pool_account = old_pool_account.clone();
            wrong_pool_account.owner = Pubkey::new_unique();
            accounts.pool_account = wrong_pool_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountOwnedByWrongProgram.into()
                )),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
            accounts.pool_account = old_pool_account;
        }

        // wrong pool authority
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let old_authority = accounts.pool_authority;
            let (bad_authority_key, _bump_seed) = Pubkey::find_program_address(
                &[b"pauthority".as_ref(), accounts.pool.as_ref()],
                &accounts.pool_token_program_id,
            );
            accounts.pool_authority = bad_authority_key;
            assert_eq!(
                Err(ProgramError::Custom(
                    SwapError::InvalidProgramAddress.into()
                )),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
            accounts.pool_authority = old_authority;
        }

        // not enough token A
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &depositor_key,
                deposit_a / 2,
                deposit_b,
                0,
            );
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
        }

        // not enough token B
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &depositor_key,
                deposit_a,
                deposit_b / 2,
                0,
            );
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
        }

        // wrong swap token accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);

            let old_token_a_program = accounts.token_a_program_id;
            let old_token_a_mint_account = accounts.token_a_mint_account;
            let old_token_a_mint_key = accounts.token_a_mint_key;
            accounts.token_a_program_id = accounts.token_b_program_id;
            accounts.token_a_mint_key = accounts.token_b_mint_key;
            accounts.token_a_mint_account = accounts.token_b_mint_account;
            accounts.token_b_program_id = old_token_a_program;
            accounts.token_b_mint_key = old_token_a_mint_key;
            accounts.token_b_mint_account = old_token_a_mint_account;
            assert_eq!(
                Err(ProgramError::Custom(AnchorError::ConstraintHasOne.into())),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_b_key,
                    &mut token_b_account,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
            let old_token_b_program = accounts.token_a_program_id;
            let old_token_b_mint_account = accounts.token_a_mint_account;
            let old_token_b_mint_key = accounts.token_a_mint_key;
            accounts.token_a_program_id = accounts.token_b_program_id;
            accounts.token_a_mint_key = accounts.token_b_mint_key;
            accounts.token_a_mint_account = accounts.token_b_mint_account;
            accounts.token_b_program_id = old_token_b_program;
            accounts.token_b_mint_key = old_token_b_mint_key;
            accounts.token_b_mint_account = old_token_b_mint_account;
        }

        // wrong pool token account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                mut _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let (
                wrong_token_key,
                mut wrong_token_account,
                _token_b_key,
                mut _token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::ConstraintTokenMint.into()
                )),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &wrong_token_key,
                    &mut wrong_token_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
        }

        // todo - elliot - delegation
        // // no approval
        // {
        //     let (
        //         token_a_key,
        //         mut token_a_account,
        //         token_b_key,
        //         mut token_b_account,
        //         pool_key,
        //         mut pool_account,
        //     ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
        //     let user_transfer_authority_key = Pubkey::new_unique();
        //     let exe = &mut SolanaAccount::default();
        //     exe.set_executable(true);
        //     assert_eq!(
        //         Err(TokenError::OwnerMismatch.into()),
        //         do_process_instruction(
        //             ix::deposit_all_token_types(
        //                 &crate::id(),
        //                 &token_a_program_id,
        //                 &token_b_program_id,
        //                 &accounts.pool_token_program_id,
        //                 &accounts.pool,
        //                 &accounts.pool_authority,
        //                 &user_transfer_authority_key,
        //                 &token_a_key,
        //                 &token_b_key,
        //                 &accounts.token_a_vault_key,
        //                 &accounts.token_b_vault_key,
        //                 &accounts.pool_token_mint_key,
        //                 &pool_key,
        //                 &accounts.token_a_mint_key,
        //                 &accounts.token_b_mint_key,
        //                 &accounts.swap_curve_key,
        //                 ix::DepositAllTokenTypes {
        //                     pool_token_amount: pool_amount.try_into().unwrap(),
        //                     maximum_token_a_amount: deposit_a,
        //                     maximum_token_b_amount: deposit_b,
        //                 },
        //             )
        //             .unwrap(),
        //             vec![
        //                 &mut accounts.pool_account,
        //                 &mut SolanaAccount::default(),
        //                 &mut SolanaAccount::default(),
        //                 &mut token_a_account,
        //                 &mut token_b_account,
        //                 &mut accounts.token_a_vault_account,
        //                 &mut accounts.token_b_vault_account,
        //                 &mut accounts.pool_token_mint_account,
        //                 &mut pool_account,
        //                 &mut accounts.token_a_mint_account,
        //                 &mut accounts.token_b_mint_account,
        //                 &mut SolanaAccount::default(),
        //                 &mut SolanaAccount::default(),
        //                 &mut SolanaAccount::default(),
        //                 &mut accounts.swap_curve_account,
        //             ],
        //         )
        //     );
        // }

        // wrong token a program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let wrong_key = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(AnchorError::InvalidProgramId.into())),
                do_process_instruction(
                    ix::deposit_all_token_types(
                        &crate::id(),
                        &wrong_key,
                        &accounts.token_b_program_id,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &token_a_key,
                        &token_b_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &accounts.pool_token_mint_key,
                        &pool_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        ix::DepositAllTokenTypes {
                            pool_token_amount: pool_amount.try_into().unwrap(),
                            maximum_token_a_amount: deposit_a,
                            maximum_token_b_amount: deposit_b,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut pool_account,
                        &mut exe.clone(),
                        &mut exe.clone(),
                        &mut exe.clone(),
                    ],
                )
            );
        }

        // wrong token b program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let wrong_key = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(AnchorError::InvalidProgramId.into())),
                do_process_instruction(
                    ix::deposit_all_token_types(
                        &crate::id(),
                        &accounts.token_a_program_id,
                        &wrong_key,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &token_a_key,
                        &token_b_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &accounts.pool_token_mint_key,
                        &pool_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        ix::DepositAllTokenTypes {
                            pool_token_amount: pool_amount.try_into().unwrap(),
                            maximum_token_a_amount: deposit_a,
                            maximum_token_b_amount: deposit_b,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut pool_account,
                        &mut exe.clone(),
                        &mut exe.clone(),
                        &mut exe.clone(),
                    ],
                )
            );
        }

        // wrong pool token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let wrong_key = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(AnchorError::InvalidProgramId.into())),
                do_process_instruction(
                    ix::deposit_all_token_types(
                        &crate::id(),
                        &accounts.token_a_program_id,
                        &accounts.token_b_program_id,
                        &wrong_key,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &token_a_key,
                        &token_b_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &accounts.pool_token_mint_key,
                        &pool_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        ix::DepositAllTokenTypes {
                            pool_token_amount: pool_amount.try_into().unwrap(),
                            maximum_token_a_amount: deposit_a,
                            maximum_token_b_amount: deposit_b,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut pool_account,
                        &mut exe.clone(),
                        &mut exe.clone(),
                        &mut exe.clone(),
                    ],
                )
            );
        }

        // wrong swap token accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);

            let old_a_key = accounts.token_a_vault_key;
            let old_a_account = accounts.token_a_vault_account;

            accounts.token_a_vault_key = token_a_key;
            accounts.token_a_vault_account = token_a_account.clone();

            // wrong swap token a vault account
            assert_eq!(
                Err(ProgramError::Custom(SwapError::IncorrectSwapAccount.into())),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );

            accounts.token_a_vault_key = old_a_key;
            accounts.token_a_vault_account = old_a_account;

            let old_b_key = accounts.token_b_vault_key;
            let old_b_account = accounts.token_b_vault_account;

            accounts.token_b_vault_key = token_b_key;
            accounts.token_b_vault_account = token_b_account.clone();

            // wrong swap token b vault account
            assert_eq!(
                Err(ProgramError::Custom(SwapError::IncorrectSwapAccount.into())),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );

            accounts.token_b_vault_key = old_b_key;
            accounts.token_b_vault_account = old_b_account;
        }

        // wrong mint
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let (pool_mint_key, pool_mint_account) = create_mint(
                &accounts.pool_token_program_id,
                &accounts.pool_authority,
                None,
                None,
                &TransferFee::default(),
            );
            let old_pool_key = accounts.pool_token_mint_key;
            let old_pool_account = accounts.pool_token_mint_account;
            accounts.pool_token_mint_key = pool_mint_key;
            accounts.pool_token_mint_account = pool_mint_account;

            assert_eq!(
                Err(ProgramError::Custom(SwapError::IncorrectPoolMint.into())),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );

            accounts.pool_token_mint_key = old_pool_key;
            accounts.pool_token_mint_account = old_pool_account;
        }

        // deposit 1 pool token fails beacuse it equates to 0 swap tokens
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            assert_eq!(
                Err(ProgramError::Custom(SwapError::ZeroTradingTokens.into())),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    1,
                    deposit_a,
                    deposit_b,
                )
            );
        }

        // slippage exceeded
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            // maximum A amount in too low
            assert_eq!(
                Err(ProgramError::Custom(SwapError::ExceededSlippage.into())),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a / 10,
                    deposit_b,
                )
            );
            // maximum B amount in too low
            assert_eq!(
                Err(ProgramError::Custom(SwapError::ExceededSlippage.into())),
                accounts.deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b / 10,
                )
            );
        }

        // invalid input: can't use swap pool tokens as source
        {
            let (
                _token_a_key,
                _token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let swap_token_a_key = accounts.token_a_vault_key;
            let mut swap_token_a_account = accounts.get_token_account(&swap_token_a_key).clone();
            let swap_token_b_key = accounts.token_b_vault_key;
            let mut swap_token_b_account = accounts.get_token_account(&swap_token_b_key).clone();
            let authority_key = accounts.pool_authority;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::ConstraintTokenOwner.into()
                )),
                accounts.deposit_all_token_types(
                    &authority_key,
                    &swap_token_a_key,
                    &mut swap_token_a_account,
                    &swap_token_b_key,
                    &mut swap_token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
            );
        }

        // correctly deposit
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            accounts
                .deposit_all_token_types(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount.try_into().unwrap(),
                    deposit_a,
                    deposit_b,
                )
                .unwrap();

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();
            assert_eq!(swap_token_a.base.amount, deposit_a + token_a_amount);
            let swap_token_b =
                StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data)
                    .unwrap();
            assert_eq!(swap_token_b.base.amount, deposit_b + token_b_amount);
            let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
            assert_eq!(token_a.base.amount, 0);
            let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
            assert_eq!(token_b.base.amount, 0);
            let pool_account = StateWithExtensions::<Account>::unpack(&pool_account.data).unwrap();
            let swap_pool_account = StateWithExtensions::<Account>::unpack(
                &accounts.admin_authority_pool_token_ata_account.data,
            )
            .unwrap();
            let pool_mint =
                StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data)
                    .unwrap();
            assert_eq!(
                pool_mint.base.supply,
                pool_account.base.amount + swap_pool_account.base.amount
            );
        }
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_withdraw(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 2;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 10;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 5;
        let host_fee_numerator = 7;
        let host_fee_denominator = 100;

        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 1000;
        let token_b_amount = 2000;
        let curve_params = CurveParameters::ConstantProduct;
        let swap_curve = SwapCurve::new_from_params(curve_params.clone());

        let withdrawer_key = Pubkey::new_unique();
        let initial_a = token_a_amount / 10;
        let initial_b = token_b_amount / 10;
        let initial_pool = swap_curve.calculator.new_pool_supply() / 10;
        let withdraw_amount = initial_pool / 4;
        let minimum_token_a_amount = initial_a / 40;
        let minimum_token_b_amount = initial_b / 40;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        // swap not initialized
        {
            let (token_a_key, mut token_a_account) = create_token_account(
                &accounts.token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &withdrawer_key,
                initial_a,
            );
            let (token_b_key, mut token_b_account) = create_token_account(
                &accounts.token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &withdrawer_key,
                initial_b,
            );
            // use token A mint because pool mint not initialized
            let (pool_key, mut pool_account) = create_token_account(
                &accounts.token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &withdrawer_key,
                0,
            );
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountDiscriminatorMismatch.into()
                )),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );
        }

        accounts.initialize_pool().unwrap();

        // wrong owner for swap account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, initial_a, initial_b, 0);
            let old_swap_account = accounts.pool_account;
            let mut wrong_swap_account = old_swap_account.clone();
            wrong_swap_account.owner = Pubkey::new_unique();
            accounts.pool_account = wrong_swap_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountOwnedByWrongProgram.into()
                )),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );
            accounts.pool_account = old_swap_account;
        }

        // wrong bump seed for authority_key
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, initial_a, initial_b, 0);
            let old_authority = accounts.pool_authority;
            let (bad_authority_key, _bump_seed) = Pubkey::find_program_address(
                &[b"pauthority".as_ref(), accounts.pool.as_ref()],
                &accounts.pool_token_program_id,
            );
            accounts.pool_authority = bad_authority_key;
            assert_eq!(
                Err(SwapError::InvalidProgramAddress.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );
            accounts.pool_authority = old_authority;
        }

        // not enough pool tokens
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                to_u64(withdraw_amount).unwrap() / 2u64,
            );
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount / 2,
                    minimum_token_b_amount / 2,
                )
            );
        }

        // wrong token a / b accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                withdraw_amount.try_into().unwrap(),
            );
            let expected_error: ProgramError = if token_a_account.owner == token_b_account.owner {
                TokenError::MintMismatch.into()
            } else if token_a_account.owner == spl_token_2022::id() {
                ProgramError::InvalidAccountData
            } else {
                // token a transfer is processed first so different error expected
                ProgramError::IncorrectProgramId
            };
            assert_eq!(
                Err(expected_error),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_b_key,
                    &mut token_b_account,
                    &token_a_key,
                    &mut token_a_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );
        }

        // wrong pool token account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                withdraw_amount.try_into().unwrap(),
            );
            let (
                wrong_token_a_key,
                mut wrong_token_a_account,
                _token_b_key,
                _token_b_account,
                _pool_key,
                pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                withdraw_amount.try_into().unwrap(),
                initial_b,
                withdraw_amount.try_into().unwrap(),
            );
            let expected_error: ProgramError = if token_a_account.owner == pool_account.owner {
                TokenError::MintMismatch.into()
            } else {
                SwapError::IncorrectTokenProgramId.into()
            };
            assert_eq!(
                Err(expected_error),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &wrong_token_a_key,
                    &mut wrong_token_a_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );
        }

        // wrong pool fee account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                wrong_pool_key,
                wrong_pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                withdraw_amount.try_into().unwrap(),
            );
            let (
                _token_a_key,
                _token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                withdraw_amount.try_into().unwrap(),
            );
            let old_pool_fee_account = accounts.pool_token_fees_vault_account;
            let old_pool_fee_key = accounts.pool_token_fees_vault_key;
            accounts.pool_token_fees_vault_account = wrong_pool_account;
            accounts.pool_token_fees_vault_key = wrong_pool_key;
            assert_eq!(
                Err(SwapError::IncorrectFeeAccount.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                ),
            );
            accounts.pool_token_fees_vault_account = old_pool_fee_account;
            accounts.pool_token_fees_vault_key = old_pool_fee_key;
        }

        // no approval
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                0,
                0,
                withdraw_amount.try_into().unwrap(),
            );
            let user_transfer_authority_key = Pubkey::new_unique();
            assert_eq!(
                Err(TokenError::OwnerMismatch.into()),
                do_process_instruction(
                    ix::withdraw_all_token_types(
                        &crate::id(),
                        &accounts.pool_token_program_id,
                        &token_a_program_id,
                        &token_b_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &user_transfer_authority_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &pool_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_a_key,
                        &token_b_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        ix::WithdrawAllTokenTypes {
                            pool_token_amount: withdraw_amount.try_into().unwrap(),
                            minimum_token_a_amount,
                            minimum_token_b_amount,
                        }
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong pool token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                withdraw_amount.try_into().unwrap(),
            );
            let wrong_key = Pubkey::new_unique();
            assert_eq!(
                Err(SwapError::IncorrectTokenProgramId.into()),
                do_process_instruction(
                    ix::withdraw_all_token_types(
                        &crate::id(),
                        &wrong_key,
                        &accounts.token_a_program_id,
                        &accounts.token_b_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &pool_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_a_key,
                        &token_b_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        ix::WithdrawAllTokenTypes {
                            pool_token_amount: withdraw_amount.try_into().unwrap(),
                            minimum_token_a_amount,
                            minimum_token_b_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong swap token accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );

            let old_a_key = accounts.token_a_vault_key;
            let old_a_account = accounts.token_a_vault_account;

            accounts.token_a_vault_key = token_a_key;
            accounts.token_a_vault_account = token_a_account.clone();

            // wrong swap token a account
            assert_eq!(
                Err(SwapError::IncorrectSwapAccount.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );

            accounts.token_a_vault_key = old_a_key;
            accounts.token_a_vault_account = old_a_account;

            let old_b_key = accounts.token_b_vault_key;
            let old_b_account = accounts.token_b_vault_account;

            accounts.token_b_vault_key = token_b_key;
            accounts.token_b_vault_account = token_b_account.clone();

            // wrong swap token b account
            assert_eq!(
                Err(SwapError::IncorrectSwapAccount.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );

            accounts.token_b_vault_key = old_b_key;
            accounts.token_b_vault_account = old_b_account;
        }

        // wrong mint
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );
            let (pool_mint_key, pool_mint_account) = create_mint(
                &accounts.pool_token_program_id,
                &accounts.pool_authority,
                None,
                None,
                &TransferFee::default(),
            );
            let old_pool_key = accounts.pool_token_mint_key;
            let old_pool_account = accounts.pool_token_mint_account;
            accounts.pool_token_mint_key = pool_mint_key;
            accounts.pool_token_mint_account = pool_mint_account;

            assert_eq!(
                Err(SwapError::IncorrectPoolMint.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );

            accounts.pool_token_mint_key = old_pool_key;
            accounts.pool_token_mint_account = old_pool_account;
        }

        // withdrawing 1 pool token fails because it equates to 0 output tokens
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );
            assert_eq!(
                Err(SwapError::ZeroTradingTokens.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    1,
                    0,
                    0,
                )
            );
        }

        // slippage exceeded
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );
            // minimum A amount out too high
            assert_eq!(
                Err(SwapError::ExceededSlippage.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount * 10,
                    minimum_token_b_amount,
                )
            );
            // minimum B amount out too high
            assert_eq!(
                Err(SwapError::ExceededSlippage.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount * 10,
                )
            );
        }

        // invalid input: can't use swap pool tokens as destination
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );
            let swap_token_a_key = accounts.token_a_vault_key;
            let mut swap_token_a_account = accounts.get_token_account(&swap_token_a_key).clone();
            assert_eq!(
                Err(SwapError::InvalidInput.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &swap_token_a_key,
                    &mut swap_token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );
            let swap_token_b_key = accounts.token_b_vault_key;
            let mut swap_token_b_account = accounts.get_token_account(&swap_token_b_key).clone();
            assert_eq!(
                Err(SwapError::InvalidInput.into()),
                accounts.withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_b_key,
                    &mut swap_token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
            );
        }

        // correct withdrawal
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );

            accounts
                .withdraw_all_token_types(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    withdraw_amount.try_into().unwrap(),
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                )
                .unwrap();

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();
            let swap_token_b =
                StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data)
                    .unwrap();
            let pool_mint =
                StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data)
                    .unwrap();
            let withdraw_fee = accounts.fees.owner_withdraw_fee(withdraw_amount).unwrap();
            let results = accounts
                .swap_curve
                .calculator
                .pool_tokens_to_trading_tokens(
                    withdraw_amount - withdraw_fee,
                    pool_mint.base.supply.try_into().unwrap(),
                    swap_token_a.base.amount.try_into().unwrap(),
                    swap_token_b.base.amount.try_into().unwrap(),
                    RoundDirection::Floor,
                )
                .unwrap();
            assert_eq!(
                swap_token_a.base.amount,
                token_a_amount - to_u64(results.token_a_amount).unwrap()
            );
            assert_eq!(
                swap_token_b.base.amount,
                token_b_amount - to_u64(results.token_b_amount).unwrap()
            );
            let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
            assert_eq!(
                token_a.base.amount,
                initial_a + to_u64(results.token_a_amount).unwrap()
            );
            let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
            assert_eq!(
                token_b.base.amount,
                initial_b + to_u64(results.token_b_amount).unwrap()
            );
            let pool_account = StateWithExtensions::<Account>::unpack(&pool_account.data).unwrap();
            assert_eq!(
                pool_account.base.amount,
                to_u64(initial_pool - withdraw_amount).unwrap()
            );
            let fee_account = StateWithExtensions::<Account>::unpack(
                &accounts.pool_token_fees_vault_account.data,
            )
            .unwrap();
            assert_eq!(
                fee_account.base.amount,
                TryInto::<u64>::try_into(withdraw_fee).unwrap()
            );
        }

        // correct withdrawal from fee account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                mut _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, 0, 0, 0);

            let pool_fee_key = accounts.pool_token_fees_vault_key;
            let mut pool_fee_account = accounts.pool_token_fees_vault_account.clone();
            let fee_account =
                StateWithExtensions::<Account>::unpack(&pool_fee_account.data).unwrap();
            let pool_fee_amount = fee_account.base.amount;

            accounts
                .withdraw_all_token_types(
                    &user_key,
                    &pool_fee_key,
                    &mut pool_fee_account,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    pool_fee_amount,
                    0,
                    0,
                )
                .unwrap();

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();
            let swap_token_b =
                StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data)
                    .unwrap();
            let pool_mint =
                StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data)
                    .unwrap();
            let results = accounts
                .swap_curve
                .calculator
                .pool_tokens_to_trading_tokens(
                    pool_fee_amount.try_into().unwrap(),
                    pool_mint.base.supply.try_into().unwrap(),
                    swap_token_a.base.amount.try_into().unwrap(),
                    swap_token_b.base.amount.try_into().unwrap(),
                    RoundDirection::Floor,
                )
                .unwrap();
            let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
            assert_eq!(
                token_a.base.amount,
                TryInto::<u64>::try_into(results.token_a_amount).unwrap()
            );
            let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
            assert_eq!(
                token_b.base.amount,
                TryInto::<u64>::try_into(results.token_b_amount).unwrap()
            );
        }
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_deposit_one_exact_in(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();
        let depositor_key = Pubkey::new_unique();
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 2;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 10;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 5;
        let host_fee_numerator = 20;
        let host_fee_denominator = 100;

        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 1000;
        let token_b_amount = 9000;
        let curve_params = CurveParameters::ConstantProduct;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        let deposit_a = token_a_amount / 10;
        let deposit_b = token_b_amount / 10;
        let pool_amount = to_u64(INITIAL_SWAP_POOL_AMOUNT / 100).unwrap();

        // swap not initialized
        {
            let (token_a_key, mut token_a_account) = create_token_account(
                &accounts.token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &depositor_key,
                deposit_a,
            );
            // use token B mint because pool mint not initialized
            let (pool_key, mut pool_account) = create_token_account(
                &accounts.token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &depositor_key,
                0,
            );
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountDiscriminatorMismatch.into()
                )),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    pool_amount,
                )
            );
        }

        accounts.initialize_pool().unwrap();

        // wrong owner for swap account
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let old_swap_account = accounts.pool_account;
            let mut wrong_swap_account = old_swap_account.clone();
            wrong_swap_account.owner = Pubkey::new_unique();
            accounts.pool_account = wrong_swap_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountOwnedByWrongProgram.into()
                )),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    pool_amount,
                )
            );
            accounts.pool_account = old_swap_account;
        }

        // wrong bump seed for authority_key
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let old_authority = accounts.pool_authority;
            let (bad_authority_key, _bump_seed) = Pubkey::find_program_address(
                &[b"pauthority".as_ref(), accounts.pool.as_ref()],
                &accounts.pool_token_program_id,
            );
            accounts.pool_authority = bad_authority_key;
            assert_eq!(
                Err(SwapError::InvalidProgramAddress.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    pool_amount,
                )
            );
            accounts.pool_authority = old_authority;
        }

        // not enough token A / B
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &depositor_key,
                deposit_a / 2,
                deposit_b / 2,
                0,
            );
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    0,
                )
            );
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_b,
                    0,
                )
            );
        }

        // wrong pool token account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let expected_error: ProgramError = if token_b_account.owner == pool_account.owner {
                TokenError::MintMismatch.into()
            } else {
                SwapError::IncorrectTokenProgramId.into()
            };
            assert_eq!(
                Err(expected_error),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    deposit_a,
                    pool_amount,
                )
            );
        }

        // no approval
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let user_transfer_authority_key = Pubkey::new_unique();
            assert_eq!(
                Err(TokenError::OwnerMismatch.into()),
                do_process_instruction(
                    ix::deposit_single_token_type_exact_amount_in(
                        &crate::id(),
                        &token_a_program_id,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &user_transfer_authority_key,
                        &token_a_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &accounts.pool_token_mint_key,
                        &pool_key,
                        &accounts.token_a_mint_key,
                        &accounts.swap_curve_key,
                        ix::DepositSingleTokenTypeExactAmountIn {
                            source_token_amount: deposit_a,
                            minimum_pool_token_amount: pool_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut token_a_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong source token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let wrong_key = Pubkey::new_unique();
            assert_eq!(
                Err(ProgramError::IncorrectProgramId),
                do_process_instruction(
                    ix::deposit_single_token_type_exact_amount_in(
                        &crate::id(),
                        &wrong_key,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &token_a_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &accounts.pool_token_mint_key,
                        &pool_key,
                        &accounts.token_a_mint_key,
                        &accounts.swap_curve_key,
                        ix::DepositSingleTokenTypeExactAmountIn {
                            source_token_amount: deposit_a,
                            minimum_pool_token_amount: pool_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut token_a_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong pool token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let wrong_key = Pubkey::new_unique();
            assert_eq!(
                Err(SwapError::IncorrectTokenProgramId.into()),
                do_process_instruction(
                    ix::deposit_single_token_type_exact_amount_in(
                        &crate::id(),
                        &token_a_program_id,
                        &wrong_key,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &token_a_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &accounts.pool_token_mint_key,
                        &pool_key,
                        &accounts.token_a_mint_key,
                        &accounts.swap_curve_key,
                        ix::DepositSingleTokenTypeExactAmountIn {
                            source_token_amount: deposit_a,
                            minimum_pool_token_amount: pool_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut token_a_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong swap token accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);

            let old_a_key = accounts.token_a_vault_key;
            let old_a_account = accounts.token_a_vault_account;

            accounts.token_a_vault_key = token_a_key;
            accounts.token_a_vault_account = token_a_account.clone();

            // wrong swap token a account
            assert_eq!(
                Err(SwapError::IncorrectSwapAccount.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    pool_amount,
                )
            );

            accounts.token_a_vault_key = old_a_key;
            accounts.token_a_vault_account = old_a_account;

            let old_b_key = accounts.token_b_vault_key;
            let old_b_account = accounts.token_b_vault_account;

            accounts.token_b_vault_key = token_b_key;
            accounts.token_b_vault_account = token_b_account;

            // wrong swap token b account
            assert_eq!(
                Err(SwapError::IncorrectSwapAccount.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    pool_amount,
                )
            );

            accounts.token_b_vault_key = old_b_key;
            accounts.token_b_vault_account = old_b_account;
        }

        // wrong mint
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let (pool_mint_key, pool_mint_account) = create_mint(
                &accounts.pool_token_program_id,
                &accounts.pool_authority,
                None,
                None,
                &TransferFee::default(),
            );
            let old_pool_key = accounts.pool_token_mint_key;
            let old_pool_account = accounts.pool_token_mint_account;
            accounts.pool_token_mint_key = pool_mint_key;
            accounts.pool_token_mint_account = pool_mint_account;

            assert_eq!(
                Err(SwapError::IncorrectPoolMint.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    pool_amount,
                )
            );

            accounts.pool_token_mint_key = old_pool_key;
            accounts.pool_token_mint_account = old_pool_account;
        }

        // slippage exceeded
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            // minimum pool amount too high
            assert_eq!(
                Err(SwapError::ExceededSlippage.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a / 10,
                    pool_amount,
                )
            );
            // minimum pool amount too high
            assert_eq!(
                Err(SwapError::ExceededSlippage.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_b / 10,
                    pool_amount,
                )
            );
        }

        // invalid input: can't use swap pool tokens as source
        {
            let (
                _token_a_key,
                _token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            let swap_token_a_key = accounts.token_a_vault_key;
            let mut swap_token_a_account = accounts.get_token_account(&swap_token_a_key).clone();
            let swap_token_b_key = accounts.token_b_vault_key;
            let mut swap_token_b_account = accounts.get_token_account(&swap_token_b_key).clone();
            let authority_key = accounts.pool_authority;
            assert_eq!(
                Err(SwapError::InvalidInput.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &authority_key,
                    &swap_token_a_key,
                    &mut swap_token_a_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_a,
                    pool_amount,
                )
            );
            assert_eq!(
                Err(SwapError::InvalidInput.into()),
                accounts.deposit_single_token_type_exact_amount_in(
                    &authority_key,
                    &swap_token_b_key,
                    &mut swap_token_b_account,
                    &pool_key,
                    &mut pool_account,
                    deposit_b,
                    pool_amount,
                )
            );
        }

        // correctly deposit
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_token_key,
                mut pool_token_account,
            ) = accounts.setup_token_accounts(&user_key, &depositor_key, deposit_a, deposit_b, 0);
            accounts
                .deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_a_key,
                    &mut token_a_account,
                    &pool_token_key,
                    &mut pool_token_account,
                    deposit_a,
                    pool_amount,
                )
                .unwrap();

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();
            assert_eq!(swap_token_a.base.amount, deposit_a + token_a_amount);

            let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
            assert_eq!(token_a.base.amount, 0);

            accounts
                .deposit_single_token_type_exact_amount_in(
                    &depositor_key,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_token_key,
                    &mut pool_token_account,
                    deposit_b,
                    pool_amount,
                )
                .unwrap();
            let swap_token_b =
                StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data)
                    .unwrap();
            assert_eq!(swap_token_b.base.amount, deposit_b + token_b_amount);

            let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
            assert_eq!(token_b.base.amount, 0);

            let pool_account =
                StateWithExtensions::<Account>::unpack(&pool_token_account.data).unwrap();
            let admin_authority_pool_token_ata = StateWithExtensions::<Account>::unpack(
                &accounts.admin_authority_pool_token_ata_account.data,
            )
            .unwrap();
            let pool_mint =
                StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data)
                    .unwrap();
            assert_eq!(
                pool_mint.base.supply,
                pool_account.base.amount + admin_authority_pool_token_ata.base.amount
            );
        }
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_withdraw_one_exact_out(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 2;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 10;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 5;
        let host_fee_numerator = 7;
        let host_fee_denominator = 100;

        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 100_000;
        let token_b_amount = 200_000;
        let curve_params = CurveParameters::ConstantProduct;
        let swap_curve = SwapCurve::new_from_params(curve_params.clone());

        let withdrawer_key = Pubkey::new_unique();
        let initial_a = token_a_amount / 10;
        let initial_b = token_b_amount / 10;
        let initial_pool = swap_curve.calculator.new_pool_supply() / 10;
        let maximum_pool_token_amount = to_u64(initial_pool / 4).unwrap();
        let destination_a_amount = initial_a / 40;
        let destination_b_amount = initial_b / 40;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        // swap not initialized
        {
            let (token_a_key, mut token_a_account) = create_token_account(
                &accounts.token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &withdrawer_key,
                initial_a,
            );
            // use token B mint because pool mint not initialized
            let (pool_key, mut pool_account) = create_token_account(
                &accounts.token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &withdrawer_key,
                0,
            );
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountDiscriminatorMismatch.into()
                )),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount,
                )
            );
        }

        accounts.initialize_pool().unwrap();

        // wrong owner for swap account
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, initial_a, initial_b, 0);
            let old_swap_account = accounts.pool_account;
            let mut wrong_swap_account = old_swap_account.clone();
            wrong_swap_account.owner = Pubkey::new_unique();
            accounts.pool_account = wrong_swap_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountOwnedByWrongProgram.into()
                )),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount,
                )
            );
            accounts.pool_account = old_swap_account;
        }

        // wrong bump seed for authority_key
        {
            let (
                _token_a_key,
                _token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, initial_a, initial_b, 0);
            let old_authority = accounts.pool_authority;
            let (bad_authority_key, _bump_seed) = Pubkey::find_program_address(
                &[b"pauthority".as_ref(), accounts.pool.as_ref()],
                &accounts.pool_token_program_id,
            );
            accounts.pool_authority = bad_authority_key;
            assert_eq!(
                Err(SwapError::InvalidProgramAddress.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_b_key,
                    &mut token_b_account,
                    destination_b_amount,
                    maximum_pool_token_amount,
                )
            );
            accounts.pool_authority = old_authority;
        }

        // not enough pool tokens
        {
            let (
                _token_a_key,
                _token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                maximum_pool_token_amount / 1000,
            );
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_b_key,
                    &mut token_b_account,
                    destination_b_amount,
                    maximum_pool_token_amount,
                )
            );
        }

        // wrong pool token account
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                maximum_pool_token_amount,
                initial_b,
                maximum_pool_token_amount,
            );
            let expected_error: ProgramError = if token_a_account.owner == pool_account.owner {
                TokenError::MintMismatch.into()
            } else {
                SwapError::IncorrectTokenProgramId.into()
            };
            assert_eq!(
                Err(expected_error),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    destination_b_amount,
                    maximum_pool_token_amount,
                )
            );
        }

        // wrong pool fee account
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                wrong_pool_key,
                wrong_pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                maximum_pool_token_amount,
            );
            let (
                _token_a_key,
                _token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                maximum_pool_token_amount,
            );
            let old_pool_fee_account = accounts.pool_token_fees_vault_account;
            let old_pool_fee_key = accounts.pool_token_fees_vault_key;
            accounts.pool_token_fees_vault_account = wrong_pool_account;
            accounts.pool_token_fees_vault_key = wrong_pool_key;
            assert_eq!(
                Err(SwapError::IncorrectFeeAccount.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount,
                )
            );
            accounts.pool_token_fees_vault_account = old_pool_fee_account;
            accounts.pool_token_fees_vault_key = old_pool_fee_key;
        }

        // no approval
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                0,
                0,
                maximum_pool_token_amount,
            );
            let user_transfer_authority_key = Pubkey::new_unique();
            assert_eq!(
                Err(TokenError::OwnerMismatch.into()),
                do_process_instruction(
                    ix::withdraw_single_token_type_exact_amount_out(
                        &crate::id(),
                        &accounts.pool_token_program_id,
                        &token_a_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &user_transfer_authority_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &pool_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_a_key,
                        &accounts.token_a_mint_key,
                        &accounts.swap_curve_key,
                        ix::WithdrawSingleTokenTypeExactAmountOut {
                            destination_token_amount: destination_a_amount,
                            maximum_pool_token_amount,
                        }
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut token_a_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut accounts.token_a_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong destination token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                maximum_pool_token_amount,
            );
            let wrong_key = Pubkey::new_unique();
            assert_eq!(
                Err(TokenError::OwnerMismatch.into()),
                do_process_instruction(
                    ix::withdraw_single_token_type_exact_amount_out(
                        &crate::id(),
                        &accounts.pool_token_program_id,
                        &wrong_key,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &pool_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_a_key,
                        &accounts.token_a_mint_key,
                        &accounts.swap_curve_key,
                        ix::WithdrawSingleTokenTypeExactAmountOut {
                            destination_token_amount: destination_a_amount,
                            maximum_pool_token_amount,
                        }
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut token_a_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut accounts.token_a_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong pool token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                maximum_pool_token_amount,
            );
            let wrong_key = Pubkey::new_unique();
            assert_eq!(
                Err(SwapError::IncorrectTokenProgramId.into()),
                do_process_instruction(
                    ix::withdraw_single_token_type_exact_amount_out(
                        &crate::id(),
                        &wrong_key,
                        &accounts.token_a_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &pool_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_a_key,
                        &accounts.token_a_mint_key,
                        &accounts.swap_curve_key,
                        ix::WithdrawSingleTokenTypeExactAmountOut {
                            destination_token_amount: destination_a_amount,
                            maximum_pool_token_amount,
                        }
                    )
                    .unwrap(),
                    vec![
                        &mut accounts.pool_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_token_mint_account,
                        &mut pool_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut token_a_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut accounts.token_a_mint_account,
                        &mut SolanaAccount::default(),
                        &mut SolanaAccount::default(),
                        &mut accounts.swap_curve_account,
                    ],
                )
            );
        }

        // wrong swap token accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );

            let old_a_key = accounts.token_a_vault_key;
            let old_a_account = accounts.token_a_vault_account;

            accounts.token_a_vault_key = token_a_key;
            accounts.token_a_vault_account = token_a_account.clone();

            // wrong swap token a account
            assert_eq!(
                Err(SwapError::IncorrectSwapAccount.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount,
                )
            );

            accounts.token_a_vault_key = old_a_key;
            accounts.token_a_vault_account = old_a_account;

            let old_b_key = accounts.token_b_vault_key;
            let old_b_account = accounts.token_b_vault_account;

            accounts.token_b_vault_key = token_b_key;
            accounts.token_b_vault_account = token_b_account.clone();

            // wrong swap token b account
            assert_eq!(
                Err(SwapError::IncorrectSwapAccount.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_b_key,
                    &mut token_b_account,
                    destination_b_amount,
                    maximum_pool_token_amount,
                )
            );

            accounts.token_b_vault_key = old_b_key;
            accounts.token_b_vault_account = old_b_account;
        }

        // wrong mint
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );
            let (pool_mint_key, pool_mint_account) = create_mint(
                &accounts.pool_token_program_id,
                &accounts.pool_authority,
                None,
                None,
                &TransferFee::default(),
            );
            let old_pool_key = accounts.pool_token_mint_key;
            let old_pool_account = accounts.pool_token_mint_account;
            accounts.pool_token_mint_key = pool_mint_key;
            accounts.pool_token_mint_account = pool_mint_account;

            assert_eq!(
                Err(SwapError::IncorrectPoolMint.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount,
                )
            );

            accounts.pool_token_mint_key = old_pool_key;
            accounts.pool_token_mint_account = old_pool_account;
        }

        // slippage exceeded
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                maximum_pool_token_amount,
            );

            // maximum pool token amount too low
            assert_eq!(
                Err(SwapError::ExceededSlippage.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount / 1000,
                )
            );
            assert_eq!(
                Err(SwapError::ExceededSlippage.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_b_key,
                    &mut token_b_account,
                    destination_b_amount,
                    maximum_pool_token_amount / 1000,
                )
            );
        }

        // invalid input: can't use swap pool tokens as destination
        {
            let (
                _token_a_key,
                _token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                maximum_pool_token_amount,
            );
            let swap_token_a_key = accounts.token_a_vault_key;
            let mut swap_token_a_account = accounts.get_token_account(&swap_token_a_key).clone();
            assert_eq!(
                Err(SwapError::InvalidInput.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &swap_token_a_key,
                    &mut swap_token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount,
                )
            );
            let swap_token_b_key = accounts.token_b_vault_key;
            let mut swap_token_b_account = accounts.get_token_account(&swap_token_b_key).clone();
            assert_eq!(
                Err(SwapError::InvalidInput.into()),
                accounts.withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &swap_token_b_key,
                    &mut swap_token_b_account,
                    destination_b_amount,
                    maximum_pool_token_amount,
                )
            );
        }

        // correct withdrawal
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(
                &user_key,
                &withdrawer_key,
                initial_a,
                initial_b,
                initial_pool.try_into().unwrap(),
            );

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();
            let swap_token_b =
                StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data)
                    .unwrap();
            let pool_mint =
                StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data)
                    .unwrap();

            let pool_token_amount = accounts
                .swap_curve
                .withdraw_single_token_type_exact_out(
                    destination_a_amount.try_into().unwrap(),
                    swap_token_a.base.amount.try_into().unwrap(),
                    swap_token_b.base.amount.try_into().unwrap(),
                    pool_mint.base.supply.try_into().unwrap(),
                    TradeDirection::AtoB,
                    &accounts.fees,
                )
                .unwrap();
            let withdraw_fee = accounts.fees.owner_withdraw_fee(pool_token_amount).unwrap();

            accounts
                .withdraw_single_token_type_exact_amount_out(
                    &withdrawer_key,
                    &pool_key,
                    &mut pool_account,
                    &token_a_key,
                    &mut token_a_account,
                    destination_a_amount,
                    maximum_pool_token_amount,
                )
                .unwrap();

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();

            assert_eq!(
                swap_token_a.base.amount,
                token_a_amount - destination_a_amount
            );
            let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
            assert_eq!(token_a.base.amount, initial_a + destination_a_amount);

            let pool_account = StateWithExtensions::<Account>::unpack(&pool_account.data).unwrap();
            assert_eq!(
                pool_account.base.amount,
                to_u64(initial_pool - pool_token_amount - withdraw_fee).unwrap()
            );
            let fee_account = StateWithExtensions::<Account>::unpack(
                &accounts.pool_token_fees_vault_account.data,
            )
            .unwrap();
            assert_eq!(fee_account.base.amount, to_u64(withdraw_fee).unwrap());
        }

        // correct withdrawal from fee account
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, initial_a, initial_b, 0);

            let fee_a_amount = 2;
            let pool_fee_key = accounts.pool_token_fees_vault_key;
            let mut pool_fee_account = accounts.pool_token_fees_vault_account.clone();
            let fee_account =
                StateWithExtensions::<Account>::unpack(&pool_fee_account.data).unwrap();
            let pool_fee_amount = fee_account.base.amount;

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();

            let token_a_amount = swap_token_a.base.amount;
            accounts
                .withdraw_single_token_type_exact_amount_out(
                    &user_key,
                    &pool_fee_key,
                    &mut pool_fee_account,
                    &token_a_key,
                    &mut token_a_account,
                    fee_a_amount,
                    pool_fee_amount,
                )
                .unwrap();

            let swap_token_a =
                StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data)
                    .unwrap();

            assert_eq!(swap_token_a.base.amount, token_a_amount - fee_a_amount);
            let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
            assert_eq!(token_a.base.amount, initial_a + fee_a_amount);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_valid_swap_curve(
        fees: Fees,
        transfer_fees: SwapTransferFees,
        curve_params: CurveParameters,
        token_a_amount: u64,
        token_b_amount: u64,
        pool_token_program_id: &Pubkey,
        token_a_program_id: &Pubkey,
        token_b_program_id: &Pubkey,
    ) {
        let user_key = Pubkey::new_unique();
        let swapper_key = Pubkey::new_unique();
        let mut token_a_amount = token_a_amount;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            transfer_fees,
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            token_a_program_id,
            token_b_program_id,
            pool_token_program_id,
        );
        // subtract the fee hit from initially depositing into the vault
        if accounts.token_a_program_id == spl_token_2022::id() {
            let token_a_init_fee = accounts
                .transfer_fees
                .token_a
                .calculate_fee(token_a_amount)
                .unwrap();
            token_a_amount -= token_a_init_fee;
        }
        let initial_a = token_a_amount / 5;
        let initial_b = token_b_amount / 5;
        accounts.initialize_pool().unwrap();

        let swap_token_a_key = accounts.token_a_vault_key;
        let swap_token_b_key = accounts.token_b_vault_key;

        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            _pool_key,
            _pool_account,
        ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
        // swap one way
        let a_to_b_amount = initial_a / 10;
        let minimum_token_b_amount = 0;
        let pool_mint =
            StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data).unwrap();
        let initial_supply = pool_mint.base.supply;
        accounts
            .swap(
                &swapper_key,
                &token_a_key,
                &mut token_a_account,
                &swap_token_a_key,
                &swap_token_b_key,
                &token_b_key,
                &mut token_b_account,
                a_to_b_amount,
                minimum_token_b_amount,
            )
            .unwrap();

        // tweak values based on transfer fees assessed
        let mut actual_a_to_b_amount = a_to_b_amount;
        if accounts.token_a_program_id == spl_token_2022::id() {
            let token_a_fee = accounts
                .transfer_fees
                .token_a
                .calculate_fee(a_to_b_amount)
                .unwrap();
            actual_a_to_b_amount = a_to_b_amount - token_a_fee;
        }
        let results = accounts
            .swap_curve
            .swap(
                actual_a_to_b_amount.try_into().unwrap(),
                token_a_amount.try_into().unwrap(),
                token_b_amount.try_into().unwrap(),
                TradeDirection::AtoB,
                &fees,
            )
            .unwrap();

        let swap_token_a =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data).unwrap();
        let token_a_amount = swap_token_a.base.amount;
        assert_eq!(
            token_a_amount,
            TryInto::<u64>::try_into(results.new_swap_source_amount).unwrap()
        );
        let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
        assert_eq!(token_a.base.amount, initial_a - a_to_b_amount);

        let swap_token_b =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data).unwrap();
        let token_b_amount = swap_token_b.base.amount;
        assert_eq!(
            token_b_amount,
            TryInto::<u64>::try_into(results.new_swap_destination_amount).unwrap()
        );
        let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
        assert_eq!(
            token_b.base.amount,
            initial_b + to_u64(results.destination_amount_swapped).unwrap()
        );

        let first_fee = if results.owner_fee > 0 {
            accounts
                .swap_curve
                .calculator
                .withdraw_single_token_type_exact_out(
                    results.owner_fee,
                    token_a_amount.try_into().unwrap(),
                    token_b_amount.try_into().unwrap(),
                    initial_supply.try_into().unwrap(),
                    TradeDirection::AtoB,
                    RoundDirection::Floor,
                )
                .unwrap()
        } else {
            0
        };
        let fee_account =
            StateWithExtensions::<Account>::unpack(&accounts.pool_token_fees_vault_account.data)
                .unwrap();
        assert_eq!(
            fee_account.base.amount,
            TryInto::<u64>::try_into(first_fee).unwrap()
        );

        let first_swap_amount = results.destination_amount_swapped;

        // swap the other way
        let pool_mint =
            StateWithExtensions::<Mint>::unpack(&accounts.pool_token_mint_account.data).unwrap();
        let initial_supply = pool_mint.base.supply;

        let b_to_a_amount = initial_b / 10;
        let minimum_a_amount = 0;
        accounts
            .swap(
                &swapper_key,
                &token_b_key,
                &mut token_b_account,
                &swap_token_b_key,
                &swap_token_a_key,
                &token_a_key,
                &mut token_a_account,
                b_to_a_amount,
                minimum_a_amount,
            )
            .unwrap();

        let mut results = accounts
            .swap_curve
            .swap(
                b_to_a_amount.try_into().unwrap(),
                token_b_amount.try_into().unwrap(),
                token_a_amount.try_into().unwrap(),
                TradeDirection::BtoA,
                &fees,
            )
            .unwrap();
        // tweak values based on transfer fees assessed
        if accounts.token_a_program_id == spl_token_2022::id() {
            let token_a_fee = accounts
                .transfer_fees
                .token_a
                .calculate_fee(results.destination_amount_swapped.try_into().unwrap())
                .unwrap();
            results.destination_amount_swapped -= token_a_fee as u128;
        }

        let swap_token_a =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data).unwrap();
        let token_a_amount = swap_token_a.base.amount;
        assert_eq!(
            token_a_amount,
            TryInto::<u64>::try_into(results.new_swap_destination_amount).unwrap()
        );
        let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
        assert_eq!(
            token_a.base.amount,
            initial_a - a_to_b_amount + to_u64(results.destination_amount_swapped).unwrap()
        );

        let swap_token_b =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data).unwrap();
        let token_b_amount = swap_token_b.base.amount;
        assert_eq!(
            token_b_amount,
            TryInto::<u64>::try_into(results.new_swap_source_amount).unwrap()
        );
        let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
        assert_eq!(
            token_b.base.amount,
            initial_b + to_u64(first_swap_amount).unwrap()
                - to_u64(results.source_amount_swapped).unwrap()
        );

        let second_fee = if results.owner_fee > 0 {
            accounts
                .swap_curve
                .calculator
                .withdraw_single_token_type_exact_out(
                    results.owner_fee,
                    token_a_amount.try_into().unwrap(),
                    token_b_amount.try_into().unwrap(),
                    initial_supply.try_into().unwrap(),
                    TradeDirection::BtoA,
                    RoundDirection::Floor,
                )
                .unwrap()
        } else {
            0
        };
        let fee_account =
            StateWithExtensions::<Account>::unpack(&accounts.pool_token_fees_vault_account.data)
                .unwrap();
        assert_eq!(
            fee_account.base.amount,
            to_u64(first_fee + second_fee).unwrap()
        );
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_valid_swap_curve_all_fees(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        // All fees
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 30;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 30;
        let host_fee_numerator = 20;
        let host_fee_denominator = 100;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 10_000_000_000;
        let token_b_amount = 50_000_000_000;

        check_valid_swap_curve(
            fees,
            SwapTransferFees::default(),
            CurveParameters::ConstantProduct,
            token_a_amount,
            token_b_amount,
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );
        let token_b_price = 1;
        check_valid_swap_curve(
            fees,
            SwapTransferFees::default(),
            CurveParameters::ConstantPrice { token_b_price },
            token_a_amount,
            token_b_amount,
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );
        let token_b_offset = 10_000_000_000;
        check_valid_swap_curve(
            fees,
            SwapTransferFees::default(),
            CurveParameters::Offset { token_b_offset },
            token_a_amount,
            token_b_amount,
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_valid_swap_curve_trade_fee_only(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 0;
        let owner_trade_fee_denominator = 0;
        let owner_withdraw_fee_numerator = 0;
        let owner_withdraw_fee_denominator = 0;
        let host_fee_numerator = 0;
        let host_fee_denominator = 0;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 10_000_000_000;
        let token_b_amount = 50_000_000_000;

        check_valid_swap_curve(
            fees,
            SwapTransferFees::default(),
            CurveParameters::ConstantProduct,
            token_a_amount,
            token_b_amount,
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );
        let token_b_price = 10_000;
        check_valid_swap_curve(
            fees,
            SwapTransferFees::default(),
            CurveParameters::ConstantPrice { token_b_price },
            token_a_amount,
            token_b_amount / token_b_price,
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );
        let token_b_offset = 1;
        check_valid_swap_curve(
            fees,
            SwapTransferFees::default(),
            CurveParameters::Offset { token_b_offset },
            token_a_amount,
            token_b_amount,
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_valid_swap_with_fee_constraints(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let owner_key = Pubkey::new_unique();

        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 30;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 30;
        let host_fee_numerator = 10;
        let host_fee_denominator = 100;

        let token_a_amount = 1_000_000;
        let token_b_amount = 5_000_000;

        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let curve_params = CurveParameters::ConstantProduct;

        let owner_key_str = &owner_key.to_string();
        let valid_curve_types = &[CurveType::ConstantProduct];
        let constraints = Some(SwapConstraints {
            owner_key: owner_key_str,
            valid_curve_types,
            fees: &fees,
        });
        let mut accounts = SwapAccountInfo::new(
            &owner_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);

        // initialize swap
        do_process_instruction_with_fee_constraints(
            ix::initialize_pool(
                &crate::id(),
                &accounts.admin_authority,
                &accounts.pool,
                &accounts.swap_curve_key,
                &accounts.token_a_mint_key,
                &accounts.token_b_mint_key,
                &accounts.token_a_vault_key,
                &accounts.token_b_vault_key,
                &accounts.pool_authority,
                &accounts.pool_token_mint_key,
                &accounts.pool_token_fees_vault_key,
                &accounts.admin_authority_token_a_ata_key,
                &accounts.admin_authority_token_b_ata_key,
                &accounts.admin_authority_pool_token_ata_key,
                &accounts.pool_token_program_id,
                &accounts.token_a_program_id,
                &accounts.token_b_program_id,
                accounts.fees,
                accounts.initial_supply.clone(),
                accounts.curve_params.clone(),
            )
            .unwrap(),
            vec![
                &mut SolanaAccount::default(),
                &mut accounts.pool_account,
                &mut accounts.swap_curve_account,
                &mut SolanaAccount::default(),
                &mut accounts.token_a_mint_account,
                &mut accounts.token_b_mint_account,
                &mut accounts.token_a_vault_account,
                &mut accounts.token_b_vault_account,
                &mut accounts.pool_token_mint_account,
                &mut accounts.pool_token_fees_vault_account,
                &mut accounts.admin_authority_token_a_ata_account,
                &mut accounts.admin_authority_token_b_ata_account,
                &mut accounts.admin_authority_pool_token_ata_account,
                &mut exe.clone(), // system_program
                &mut create_account_for_test(&Rent::default()),
                &mut exe.clone(), // pool_token_program
                &mut exe.clone(), // token_a_program
                &mut exe.clone(), // token_b_program
            ],
            &constraints,
        )
        .unwrap();

        let authority_key = accounts.pool_authority;

        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            pool_key,
            mut pool_account,
        ) = accounts.setup_token_accounts(
            &owner_key,
            &authority_key,
            token_a_amount,
            token_b_amount,
            0,
        );

        let amount_in = token_a_amount / 2;
        let minimum_amount_out = 0;

        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);

        // perform the swap
        do_process_instruction_with_fee_constraints(
            ix::swap(
                &crate::id(),
                &token_a_program_id,
                &token_b_program_id,
                &accounts.pool_token_program_id,
                &accounts.pool,
                &accounts.pool_authority,
                &accounts.pool_authority,
                &token_a_key,
                &accounts.token_a_vault_key,
                &accounts.token_b_vault_key,
                &token_b_key,
                &accounts.pool_token_mint_key,
                &accounts.pool_token_fees_vault_key,
                &accounts.token_a_mint_key,
                &accounts.token_b_mint_key,
                &accounts.swap_curve_key,
                Some(&pool_key),
                ix::Swap {
                    amount_in,
                    minimum_amount_out,
                },
            )
            .unwrap(),
            vec![
                &mut SolanaAccount::default(),
                &mut accounts.pool_account,
                &mut accounts.swap_curve_account,
                &mut SolanaAccount::default(),
                &mut accounts.token_a_mint_account,
                &mut accounts.token_b_mint_account,
                &mut accounts.token_a_vault_account,
                &mut accounts.token_b_vault_account,
                &mut accounts.pool_token_mint_account,
                &mut accounts.pool_token_fees_vault_account,
                &mut token_a_account,
                &mut token_b_account,
                &mut pool_account,
                &mut exe.clone(), // pool_token_program
                &mut exe.clone(), // source_token_program
                &mut exe.clone(), // destination_token_program
            ],
            &constraints,
        )
        .unwrap();

        // check that fees were taken in the host fee account
        let host_fee_account = StateWithExtensions::<Account>::unpack(&pool_account.data).unwrap();
        let owner_fee_account =
            StateWithExtensions::<Account>::unpack(&accounts.pool_token_fees_vault_account.data)
                .unwrap();
        let total_fee = owner_fee_account.base.amount * host_fee_denominator
            / (host_fee_denominator - host_fee_numerator);
        assert_eq!(
            total_fee,
            host_fee_account.base.amount + owner_fee_account.base.amount
        );
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_invalid_swap(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();
        let swapper_key = Pubkey::new_unique();
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 4;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 10;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 5;
        let host_fee_numerator = 9;
        let host_fee_denominator = 100;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 1000;
        let token_b_amount = 5000;
        let curve_params = CurveParameters::ConstantProduct;
        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        let initial_a = token_a_amount / 5;
        let initial_b = token_b_amount / 5;
        let minimum_token_b_amount = initial_b / 2;

        let swap_token_a_key = accounts.token_a_vault_key;
        let swap_token_b_key = accounts.token_b_vault_key;

        // swap not initialized
        {
            let (token_a_key, mut token_a_account) = create_token_account(
                &accounts.token_a_program_id,
                &accounts.token_a_mint_key,
                &mut accounts.token_a_mint_account,
                &user_key,
                &swapper_key,
                initial_a,
            );
            let (token_b_key, mut token_b_account) = create_token_account(
                &accounts.token_b_program_id,
                &accounts.token_b_mint_key,
                &mut accounts.token_b_mint_account,
                &user_key,
                &swapper_key,
                initial_b,
            );
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountDiscriminatorMismatch.into()
                )),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_a_key,
                    &swap_token_b_key,
                    &token_b_key,
                    &mut token_b_account,
                    initial_a,
                    minimum_token_b_amount,
                )
            );
        }

        accounts.initialize_pool().unwrap();

        // wrong swap account program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            let old_swap_account = accounts.pool_account;
            let mut wrong_swap_account = old_swap_account.clone();
            wrong_swap_account.owner = Pubkey::new_unique();
            accounts.pool_account = wrong_swap_account;
            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::AccountOwnedByWrongProgram.into()
                )),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_a_key,
                    &swap_token_b_key,
                    &token_b_key,
                    &mut token_b_account,
                    initial_a,
                    minimum_token_b_amount,
                )
            );
            accounts.pool_account = old_swap_account;
        }

        // wrong pool authority
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            let old_authority = accounts.pool_authority;
            let (bad_authority_key, _bump_seed) = Pubkey::find_program_address(
                &[b"pauthority".as_ref(), accounts.pool.as_ref()],
                &accounts.pool_token_program_id,
            );
            accounts.pool_authority = bad_authority_key;
            assert_eq!(
                Err(ProgramError::Custom(
                    SwapError::InvalidProgramAddress.into()
                )),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_a_key,
                    &swap_token_b_key,
                    &token_b_key,
                    &mut token_b_account,
                    initial_a,
                    minimum_token_b_amount,
                )
            );
            accounts.pool_authority = old_authority;
        }

        // wrong source token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            // approve moving from user source account
            let user_transfer_key = Pubkey::new_unique();
            do_process_instruction(
                approve(
                    &accounts.token_a_program_id,
                    &token_a_key,
                    &user_transfer_key,
                    &swapper_key,
                    &[],
                    initial_a,
                )
                .unwrap(),
                vec![
                    &mut token_a_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                ],
            )
            .unwrap();
            let wrong_program_id = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(AnchorError::InvalidProgramId.into())),
                do_process_instruction(
                    ix::swap(
                        &crate::id(),
                        &wrong_program_id,
                        &accounts.token_b_program_id,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &user_transfer_key,
                        &token_a_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_b_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        None,
                        ix::Swap {
                            amount_in: initial_a,
                            minimum_amount_out: minimum_token_b_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut exe.clone(), // pool_token_program
                        &mut exe.clone(), // source_token_program
                        &mut exe.clone(), // destination_token_program
                    ],
                ),
            );
        }

        // wrong destination token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            // approve moving from user source account
            let user_transfer_key = Pubkey::new_unique();
            do_process_instruction(
                approve(
                    &accounts.token_a_program_id,
                    &token_a_key,
                    &user_transfer_key,
                    &swapper_key,
                    &[],
                    initial_a,
                )
                .unwrap(),
                vec![
                    &mut token_a_account,
                    &mut SolanaAccount::default(),
                    &mut SolanaAccount::default(),
                ],
            )
            .unwrap();
            let wrong_program_id = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(AnchorError::InvalidProgramId.into())),
                do_process_instruction(
                    ix::swap(
                        &crate::id(),
                        &accounts.token_a_program_id,
                        &wrong_program_id,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &user_transfer_key,
                        &token_a_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_b_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        None,
                        ix::Swap {
                            amount_in: initial_a,
                            minimum_amount_out: minimum_token_b_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut exe.clone(), // Optional front end host fees - passed as the program if not present
                        &mut exe.clone(), // pool_token_program
                        &mut exe.clone(), // source_token_program
                        &mut exe.clone(), // destination_token_program
                    ],
                ),
            );
        }

        // wrong pool token program id
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            let wrong_program_id = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(AnchorError::InvalidProgramId.into())),
                do_process_instruction(
                    ix::swap(
                        &crate::id(),
                        &accounts.token_a_program_id,
                        &accounts.token_b_program_id,
                        &wrong_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority, // not used
                        &token_a_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_b_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        None,
                        ix::Swap {
                            amount_in: initial_a,
                            minimum_amount_out: minimum_token_b_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut exe.clone(), // pool_token_program
                        &mut exe.clone(), // source_token_program
                        &mut exe.clone(), // destination_token_program
                    ],
                ),
            );
        }

        // not enough token a to swap
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            assert_eq!(
                Err(TokenError::InsufficientFunds.into()),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_a_key,
                    &swap_token_b_key,
                    &token_b_key,
                    &mut token_b_account,
                    initial_a * 2,
                    minimum_token_b_amount * 2,
                )
            );
        }

        // wrong swap token A / B accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            let user_transfer_key = Pubkey::new_unique();

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::ConstraintTokenOwner.into()
                )),
                do_process_instruction(
                    ix::swap(
                        &crate::id(),
                        &token_a_program_id,
                        &token_b_program_id,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &user_transfer_key, // todo - elliot - delegation
                        &token_a_key,
                        &token_a_key,
                        &token_b_key,
                        &token_b_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        None,
                        ix::Swap {
                            amount_in: initial_a,
                            minimum_amount_out: minimum_token_b_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut token_a_account.clone(),
                        &mut token_b_account.clone(),
                        &mut accounts.pool_token_mint_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut exe.clone(), // Optional front end host fees - passed as the program if not present
                        &mut exe.clone(), // pool_token_program
                        &mut exe.clone(), // source_token_program
                        &mut exe.clone(), // destination_token_program
                    ],
                ),
            );
        }

        // wrong user token A / B accounts
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::ConstraintTokenMint.into()
                )),
                do_process_instruction(
                    ix::swap(
                        &crate::id(),
                        &accounts.token_a_program_id,
                        &accounts.token_b_program_id,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &swapper_key,
                        &token_b_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_a_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        None,
                        ix::Swap {
                            amount_in: initial_a,
                            minimum_amount_out: minimum_token_b_amount,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut token_b_account,
                        &mut token_a_account,
                        &mut exe.clone(), // Optional front end host fees - passed as the program if not present
                        &mut exe.clone(), // pool_token_program
                        &mut exe.clone(), // source_token_program
                        &mut exe.clone(), // destination_token_program
                    ],
                ),
            );
        }

        // swap from a to a
        {
            let (
                token_a_key,
                mut token_a_account,
                _token_b_key,
                _token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            assert_eq!(
                Err(ProgramError::Custom(SwapError::RepeatedMint.into())),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account.clone(),
                    &swap_token_a_key,
                    &swap_token_a_key,
                    &token_a_key,
                    &mut token_a_account,
                    initial_a,
                    minimum_token_b_amount,
                )
            );
        }

        // incorrect mint provided
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            let (pool_mint_key, pool_mint_account) = create_mint(
                &accounts.pool_token_program_id,
                &accounts.pool_authority,
                None,
                None,
                &TransferFee::default(),
            );
            let old_pool_key = accounts.pool_token_mint_key;
            let old_pool_account = accounts.pool_token_mint_account;
            accounts.pool_token_mint_key = pool_mint_key;
            accounts.pool_token_mint_account = pool_mint_account;

            assert_eq!(
                Err(ProgramError::Custom(SwapError::IncorrectPoolMint.into())),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_a_key,
                    &swap_token_b_key,
                    &token_b_key,
                    &mut token_b_account,
                    initial_a,
                    minimum_token_b_amount,
                )
            );

            accounts.pool_token_mint_key = old_pool_key;
            accounts.pool_token_mint_account = old_pool_account;
        }

        // incorrect fee account provided
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                wrong_pool_key,
                wrong_pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            let old_pool_fee_account = accounts.pool_token_fees_vault_account;
            let old_pool_fee_key = accounts.pool_token_fees_vault_key;
            accounts.pool_token_fees_vault_account = wrong_pool_account;
            accounts.pool_token_fees_vault_key = wrong_pool_key;
            assert_eq!(
                Err(ProgramError::Custom(SwapError::IncorrectFeeAccount.into())),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_a_key,
                    &swap_token_b_key,
                    &token_b_key,
                    &mut token_b_account,
                    initial_a,
                    minimum_token_b_amount,
                )
            );
            accounts.pool_token_fees_vault_account = old_pool_fee_account;
            accounts.pool_token_fees_vault_key = old_pool_fee_key;
        }

        // todo - elliot - delegation
        // no approval
        // {
        //     let (
        //         token_a_key,
        //         mut token_a_account,
        //         token_b_key,
        //         mut token_b_account,
        //         _pool_key,
        //         _pool_account,
        //     ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
        //     let user_transfer_key = Pubkey::new_unique();
        //
        //     let exe = &mut SolanaAccount::default();
        //     exe.set_executable(true);
        //
        //     assert_eq!(
        //         Err(TokenError::OwnerMismatch.into()),
        //         do_process_instruction(
        //             ix::swap(
        //                 &crate::id(),
        //                 &token_a_program_id,
        //                 &token_b_program_id,
        //                 &accounts.pool_token_program_id,
        //                 &accounts.pool,
        //                 &accounts.pool_authority,
        //                 &user_transfer_key,
        //                 &token_a_key,
        //                 &accounts.token_a_vault_key,
        //                 &accounts.token_b_vault_key,
        //                 &token_b_key,
        //                 &accounts.pool_token_mint_key,
        //                 &accounts.pool_token_fees_vault_key,
        //                 &accounts.token_a_mint_key,
        //                 &accounts.token_b_mint_key,
        //                 &accounts.swap_curve_key,
        //                 None,
        //                 ix::Swap {
        //                     amount_in: initial_a,
        //                     minimum_amount_out: minimum_token_b_amount,
        //                 },
        //             )
        //             .unwrap(),
        //             vec![
        //                 &mut SolanaAccount::default(),
        //                 &mut accounts.pool_account,
        //                 &mut accounts.swap_curve_account,
        //                 &mut SolanaAccount::default(),
        //                 &mut accounts.token_a_mint_account,
        //                 &mut accounts.token_b_mint_account,
        //                 &mut accounts.token_a_vault_account,
        //                 &mut accounts.token_b_vault_account,
        //                 &mut accounts.pool_token_mint_account,
        //                 &mut accounts.pool_token_fees_vault_account,
        //                 &mut token_a_account,
        //                 &mut token_b_account,
        //                 &mut exe.clone(), // Optional front end host fees - passed as the program if not present
        //                 &mut exe.clone(), // pool_token_program
        //                 &mut exe.clone(), // source_token_program
        //                 &mut exe.clone(), // destination_token_program
        //             ],
        //         ),
        //     );
        // }

        // output token value 0
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            assert_eq!(
                Err(ProgramError::Custom(SwapError::ZeroTradingTokens.into())),
                accounts.swap(
                    &swapper_key,
                    &token_b_key,
                    &mut token_b_account,
                    &swap_token_b_key,
                    &swap_token_a_key,
                    &token_a_key,
                    &mut token_a_account,
                    1,
                    1,
                )
            );
        }

        // slippage exceeded: minimum out amount too high
        {
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            assert_eq!(
                Err(ProgramError::Custom(SwapError::ExceededSlippage.into())),
                accounts.swap(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &swap_token_a_key,
                    &swap_token_b_key,
                    &token_b_key,
                    &mut token_b_account,
                    initial_a,
                    minimum_token_b_amount * 2,
                )
            );
        }

        // todo - elliot - remove as authority is PDA unique to pool?
        // invalid input: can't use swap pool vault as user source / dest
        // {
        //     let authority_key = accounts.pool_authority;
        //     let (
        //         token_a_key,
        //         mut token_a_account,
        //         token_b_key,
        //         mut token_b_account,
        //         _pool_key,
        //         _pool_account,
        //     ) = accounts.setup_token_accounts(&user_key, &authority_key, initial_a, initial_b, 0);
        //     let mut swap_token_a_account = accounts.get_token_account(&swap_token_a_key).clone();
        //     assert_eq!(
        //         Err(SwapError::InvalidInput.into()),
        //         accounts.swap(
        //             &authority_key,
        //             &swap_token_a_key,
        //             &mut swap_token_a_account,
        //             &swap_token_a_key,
        //             &swap_token_b_key,
        //             &token_b_key,
        //             &mut token_b_account,
        //             initial_a,
        //             minimum_token_b_amount,
        //         )
        //     );
        //     let mut swap_token_b_account = accounts.get_token_account(&swap_token_b_key).clone();
        //     assert_eq!(
        //         Err(SwapError::InvalidInput.into()),
        //         accounts.swap(
        //             &swapper_key,
        //             &token_a_key,
        //             &mut token_a_account,
        //             &swap_token_a_key,
        //             &swap_token_b_key,
        //             &swap_token_b_key,
        //             &mut swap_token_b_account,
        //             initial_a,
        //             minimum_token_b_amount,
        //         )
        //     );
        // }

        // still correct: constraint specified, no host fee account
        {
            let authority_key = accounts.pool_authority;
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &authority_key, initial_a, initial_b, 0);
            let owner_key = &swapper_key.to_string();
            let fees = Fees {
                trade_fee_numerator,
                trade_fee_denominator,
                owner_trade_fee_numerator,
                owner_trade_fee_denominator,
                owner_withdraw_fee_numerator,
                owner_withdraw_fee_denominator,
                host_fee_numerator,
                host_fee_denominator,
            };
            let constraints = Some(SwapConstraints {
                owner_key,
                valid_curve_types: &[],
                fees: &fees,
            });

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            do_process_instruction_with_fee_constraints(
                ix::swap(
                    &crate::id(),
                    &token_a_program_id,
                    &token_b_program_id,
                    &accounts.pool_token_program_id,
                    &accounts.pool,
                    &accounts.pool_authority,
                    &accounts.pool_authority,
                    &token_a_key,
                    &accounts.token_a_vault_key,
                    &accounts.token_b_vault_key,
                    &token_b_key,
                    &accounts.pool_token_mint_key,
                    &accounts.pool_token_fees_vault_key,
                    &accounts.token_a_mint_key,
                    &accounts.token_b_mint_key,
                    &accounts.swap_curve_key,
                    None,
                    ix::Swap {
                        amount_in: initial_a,
                        minimum_amount_out: minimum_token_b_amount,
                    },
                )
                .unwrap(),
                vec![
                    &mut SolanaAccount::default(),
                    &mut accounts.pool_account,
                    &mut accounts.swap_curve_account,
                    &mut SolanaAccount::default(),
                    &mut accounts.token_a_mint_account,
                    &mut accounts.token_b_mint_account,
                    &mut accounts.token_a_vault_account,
                    &mut accounts.token_b_vault_account,
                    &mut accounts.pool_token_mint_account,
                    &mut accounts.pool_token_fees_vault_account,
                    &mut token_a_account,
                    &mut token_b_account,
                    &mut exe.clone(), // Optional front end host fees - passed as the program if not present
                    &mut exe.clone(), // pool_token_program
                    &mut exe.clone(), // source_token_program
                    &mut exe.clone(), // destination_token_program
                ],
                &constraints,
            )
            .unwrap();
        }

        // invalid mint for host fee account
        {
            let authority_key = accounts.pool_authority;
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &authority_key, initial_a, initial_b, 0);
            let (
                bad_token_a_key,
                mut bad_token_a_account,
                _token_b_key,
                mut _token_b_account,
                _pool_key,
                _pool_account,
            ) = accounts.setup_token_accounts(&user_key, &authority_key, initial_a, initial_b, 0);
            let owner_key = &swapper_key.to_string();
            let fees = Fees {
                trade_fee_numerator,
                trade_fee_denominator,
                owner_trade_fee_numerator,
                owner_trade_fee_denominator,
                owner_withdraw_fee_numerator,
                owner_withdraw_fee_denominator,
                host_fee_numerator,
                host_fee_denominator,
            };
            let constraints = Some(SwapConstraints {
                owner_key,
                valid_curve_types: &[],
                fees: &fees,
            });

            let exe = &mut SolanaAccount::default();
            exe.set_executable(true);

            assert_eq!(
                Err(ProgramError::Custom(
                    AnchorError::ConstraintTokenMint.into()
                )),
                do_process_instruction_with_fee_constraints(
                    ix::swap(
                        &crate::id(),
                        &token_a_program_id,
                        &token_b_program_id,
                        &accounts.pool_token_program_id,
                        &accounts.pool,
                        &accounts.pool_authority,
                        &accounts.pool_authority,
                        &token_a_key,
                        &accounts.token_a_vault_key,
                        &accounts.token_b_vault_key,
                        &token_b_key,
                        &accounts.pool_token_mint_key,
                        &accounts.pool_token_fees_vault_key,
                        &accounts.token_a_mint_key,
                        &accounts.token_b_mint_key,
                        &accounts.swap_curve_key,
                        Some(&bad_token_a_key),
                        ix::Swap {
                            amount_in: initial_a,
                            minimum_amount_out: 0,
                        },
                    )
                    .unwrap(),
                    vec![
                        &mut SolanaAccount::default(),
                        &mut accounts.pool_account,
                        &mut accounts.swap_curve_account,
                        &mut SolanaAccount::default(),
                        &mut accounts.token_a_mint_account,
                        &mut accounts.token_b_mint_account,
                        &mut accounts.token_a_vault_account,
                        &mut accounts.token_b_vault_account,
                        &mut accounts.pool_token_mint_account,
                        &mut accounts.pool_token_fees_vault_account,
                        &mut token_a_account,
                        &mut token_b_account,
                        &mut bad_token_a_account, // Optional front end host fees - passed as the program if not present
                        &mut exe.clone(),         // pool_token_program
                        &mut exe.clone(),         // source_token_program
                        &mut exe.clone(),         // destination_token_program
                    ],
                    &constraints,
                ),
            );
        }
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_overdraw_offset_curve(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 30;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 30;
        let host_fee_numerator = 10;
        let host_fee_denominator = 100;

        let token_a_amount = 1_000_000_000;
        let token_b_amount = 0;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_b_offset = 2_000_000;
        let curve_params = CurveParameters::Offset { token_b_offset };
        let user_key = Pubkey::new_unique();
        let swapper_key = Pubkey::new_unique();

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        accounts.initialize_pool().unwrap();

        let swap_token_a_key = accounts.token_a_vault_key;
        let swap_token_b_key = accounts.token_b_vault_key;
        let initial_a = 500_000;
        let initial_b = 1_000;

        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            _pool_key,
            _pool_account,
        ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);

        // swap a to b way, fails, there's no liquidity
        let a_to_b_amount = initial_a;
        let minimum_token_b_amount = 0;

        assert_eq!(
            Err(ProgramError::Custom(SwapError::ZeroTradingTokens.into())),
            accounts.swap(
                &swapper_key,
                &token_a_key,
                &mut token_a_account,
                &swap_token_a_key,
                &swap_token_b_key,
                &token_b_key,
                &mut token_b_account,
                a_to_b_amount,
                minimum_token_b_amount,
            )
        );

        // swap b to a, succeeds at offset price
        let b_to_a_amount = initial_b;
        let minimum_token_a_amount = 0;
        accounts
            .swap(
                &swapper_key,
                &token_b_key,
                &mut token_b_account,
                &swap_token_b_key,
                &swap_token_a_key,
                &token_a_key,
                &mut token_a_account,
                b_to_a_amount,
                minimum_token_a_amount,
            )
            .unwrap();

        // try a to b again, succeeds due to new liquidity
        accounts
            .swap(
                &swapper_key,
                &token_a_key,
                &mut token_a_account,
                &swap_token_a_key,
                &swap_token_b_key,
                &token_b_key,
                &mut token_b_account,
                a_to_b_amount,
                minimum_token_b_amount,
            )
            .unwrap();

        // try a to b again, fails due to no more liquidity
        assert_eq!(
            Err(ProgramError::Custom(SwapError::ZeroTradingTokens.into())),
            accounts.swap(
                &swapper_key,
                &token_a_key,
                &mut token_a_account,
                &swap_token_a_key,
                &swap_token_b_key,
                &token_b_key,
                &mut token_b_account,
                a_to_b_amount,
                minimum_token_b_amount,
            )
        );

        // Try to deposit, fails because deposits are not allowed for offset
        // curve swaps
        {
            let initial_a = 100;
            let initial_b = 100;
            let pool_amount = 100;
            let (
                token_a_key,
                mut token_a_account,
                token_b_key,
                mut token_b_account,
                pool_key,
                mut pool_account,
            ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
            assert_eq!(
                Err(ProgramError::Custom(
                    SwapError::UnsupportedCurveOperation.into()
                )),
                accounts.deposit_all_token_types(
                    &swapper_key,
                    &token_a_key,
                    &mut token_a_account,
                    &token_b_key,
                    &mut token_b_account,
                    &pool_key,
                    &mut pool_account,
                    pool_amount,
                    initial_a,
                    initial_b,
                )
            );
        }
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_withdraw_all_offset_curve(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 30;
        let owner_withdraw_fee_numerator = 0;
        let owner_withdraw_fee_denominator = 30;
        let host_fee_numerator = 10;
        let host_fee_denominator = 100;

        let token_a_amount = 1_000_000_000;
        let token_b_amount = 10;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_b_offset = 2_000_000;
        let curve_params = CurveParameters::Offset { token_b_offset };
        let swap_curve = SwapCurve::new_from_params(curve_params.clone());
        let total_pool = swap_curve.calculator.new_pool_supply();
        let user_key = Pubkey::new_unique();
        let withdrawer_key = Pubkey::new_unique();

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        accounts.initialize_pool().unwrap();

        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            _pool_key,
            _pool_account,
        ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, 0, 0, 0);

        let pool_key = accounts.admin_authority_pool_token_ata_key;
        let mut pool_account = accounts.admin_authority_pool_token_ata_account.clone();

        // WithdrawAllTokenTypes takes all tokens for A and B.
        // The curve's calculation for token B will say to transfer
        // `token_b_offset + token_b_amount`, but only `token_b_amount` will be
        // moved.
        accounts
            .withdraw_all_token_types(
                &user_key,
                &pool_key,
                &mut pool_account,
                &token_a_key,
                &mut token_a_account,
                &token_b_key,
                &mut token_b_account,
                total_pool.try_into().unwrap(),
                0,
                0,
            )
            .unwrap();

        let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
        assert_eq!(token_a.base.amount, token_a_amount);
        let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
        assert_eq!(token_b.base.amount, token_b_amount);
        let swap_token_a =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data).unwrap();
        assert_eq!(swap_token_a.base.amount, 0);
        let swap_token_b =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data).unwrap();
        assert_eq!(swap_token_b.base.amount, 0);
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_withdraw_all_constant_price_curve(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 30;
        let owner_withdraw_fee_numerator = 0;
        let owner_withdraw_fee_denominator = 30;
        let host_fee_numerator = 10;
        let host_fee_denominator = 100;

        // initialize "unbalanced", so that withdrawing all will have some issues
        // A: 1_000_000_000
        // B: 2_000_000_000 (1_000 * 2_000_000)
        let swap_token_a_amount = 1_000_000_000;
        let swap_token_b_amount = 1_000;
        let token_b_price = 2_000_000;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let curve_params = CurveParameters::ConstantPrice { token_b_price };
        let swap_curve = SwapCurve::new_from_params(curve_params.clone());
        let total_pool = swap_curve.calculator.new_pool_supply();
        let user_key = Pubkey::new_unique();
        let withdrawer_key = Pubkey::new_unique();

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: swap_token_a_amount,
                initial_supply_b: swap_token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        accounts.initialize_pool().unwrap();

        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            _pool_key,
            _pool_account,
        ) = accounts.setup_token_accounts(&user_key, &withdrawer_key, 0, 0, 0);

        let pool_key = accounts.admin_authority_pool_token_ata_key;
        let mut pool_account = accounts.admin_authority_pool_token_ata_account.clone();

        // WithdrawAllTokenTypes will not take all token A and B, since their
        // ratio is unbalanced.  It will try to take 1_500_000_000 worth of
        // each token, which means 1_500_000_000 token A, and 750 token B.
        // With no slippage, this will leave 250 token B in the pool.
        assert_eq!(
            Err(SwapError::ExceededSlippage.into()),
            accounts.withdraw_all_token_types(
                &user_key,
                &pool_key,
                &mut pool_account,
                &token_a_key,
                &mut token_a_account,
                &token_b_key,
                &mut token_b_account,
                total_pool.try_into().unwrap(),
                swap_token_a_amount,
                swap_token_b_amount,
            )
        );

        accounts
            .withdraw_all_token_types(
                &user_key,
                &pool_key,
                &mut pool_account,
                &token_a_key,
                &mut token_a_account,
                &token_b_key,
                &mut token_b_account,
                total_pool.try_into().unwrap(),
                0,
                0,
            )
            .unwrap();

        let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
        assert_eq!(token_a.base.amount, swap_token_a_amount);
        let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
        assert_eq!(token_b.base.amount, 750);
        let swap_token_a =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data).unwrap();
        assert_eq!(swap_token_a.base.amount, 0);
        let swap_token_b =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data).unwrap();
        assert_eq!(swap_token_b.base.amount, 250);

        // deposit now, not enough to cover the tokens already in there
        let token_b_amount = 10;
        let token_a_amount = token_b_amount * token_b_price;
        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            pool_key,
            mut pool_account,
        ) = accounts.setup_token_accounts(
            &user_key,
            &withdrawer_key,
            token_a_amount,
            token_b_amount,
            0,
        );

        assert_eq!(
            Err(ProgramError::Custom(SwapError::ExceededSlippage.into())),
            accounts.deposit_all_token_types(
                &withdrawer_key,
                &token_a_key,
                &mut token_a_account,
                &token_b_key,
                &mut token_b_account,
                &pool_key,
                &mut pool_account,
                1, // doesn't matter
                token_a_amount,
                token_b_amount,
            )
        );

        // deposit enough tokens, success!
        let token_b_amount = 125;
        let token_a_amount = token_b_amount * token_b_price;
        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            pool_key,
            mut pool_account,
        ) = accounts.setup_token_accounts(
            &user_key,
            &withdrawer_key,
            token_a_amount,
            token_b_amount,
            0,
        );

        accounts
            .deposit_all_token_types(
                &withdrawer_key,
                &token_a_key,
                &mut token_a_account,
                &token_b_key,
                &mut token_b_account,
                &pool_key,
                &mut pool_account,
                1, // doesn't matter
                token_a_amount,
                token_b_amount,
            )
            .unwrap();
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_deposits_allowed_single_token(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 30;
        let owner_withdraw_fee_numerator = 0;
        let owner_withdraw_fee_denominator = 30;
        let host_fee_numerator = 10;
        let host_fee_denominator = 100;

        let token_a_amount = 1_000_000;
        let token_b_amount = 0;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_b_offset = 2_000_000;
        let swap_curve = CurveParameters::Offset { token_b_offset };
        let creator_key = Pubkey::new_unique();
        let depositor_key = Pubkey::new_unique();

        let mut accounts = SwapAccountInfo::new(
            &creator_key,
            fees,
            SwapTransferFees::default(),
            swap_curve,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        accounts.initialize_pool().unwrap();

        let initial_a = 1_000_000;
        let initial_b = 2_000_000;
        let (
            _depositor_token_a_key,
            _depositor_token_a_account,
            depositor_token_b_key,
            mut depositor_token_b_account,
            depositor_pool_key,
            mut depositor_pool_account,
        ) = accounts.setup_token_accounts(&creator_key, &depositor_key, initial_a, initial_b, 0);

        assert_eq!(
            Err(SwapError::UnsupportedCurveOperation.into()),
            accounts.deposit_single_token_type_exact_amount_in(
                &depositor_key,
                &depositor_token_b_key,
                &mut depositor_token_b_account,
                &depositor_pool_key,
                &mut depositor_pool_account,
                initial_b,
                0,
            )
        );
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_withdraw_with_invalid_fee_account(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();

        let fees = Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 2,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 10,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 5,
            host_fee_numerator: 7,
            host_fee_denominator: 100,
        };

        let token_a_amount = 1000;
        let token_b_amount = 2000;
        let curve_params = CurveParameters::ConstantProduct;
        let swap_curve = SwapCurve::new_from_params(curve_params.clone());

        let withdrawer_key = Pubkey::new_unique();
        let initial_a = token_a_amount / 10;
        let initial_b = token_b_amount / 10;
        let initial_pool = swap_curve.calculator.new_pool_supply() / 10;
        let withdraw_amount = initial_pool / 4;
        let minimum_token_a_amount = initial_a / 40;
        let minimum_token_b_amount = initial_b / 40;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        accounts.initialize_pool().unwrap();

        let (
            token_a_key,
            mut token_a_account,
            token_b_key,
            mut token_b_account,
            pool_key,
            mut pool_account,
        ) = accounts.setup_token_accounts(
            &user_key,
            &withdrawer_key,
            initial_a,
            initial_b,
            initial_pool.try_into().unwrap(),
        );

        let destination_key = Pubkey::new_unique();
        let mut destination = SolanaAccount::new(
            account_minimum_balance(),
            Account::get_packed_len(),
            &withdrawer_key,
        );

        do_process_instruction(
            close_account(
                &accounts.pool_token_program_id,
                &accounts.pool_token_fees_vault_key,
                &destination_key,
                &user_key,
                &[],
            )
            .unwrap(),
            vec![
                &mut accounts.pool_token_fees_vault_account,
                &mut destination,
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        let user_transfer_authority_key = Pubkey::new_unique();
        let pool_token_amount = withdraw_amount.try_into().unwrap();

        do_process_instruction(
            approve(
                &accounts.pool_token_program_id,
                &pool_key,
                &user_transfer_authority_key,
                &withdrawer_key,
                &[],
                pool_token_amount,
            )
            .unwrap(),
            vec![
                &mut pool_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        do_process_instruction(
            ix::withdraw_all_token_types(
                &crate::id(),
                &accounts.pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
                &accounts.pool,
                &accounts.pool_authority,
                &user_transfer_authority_key,
                &accounts.pool_token_mint_key,
                &accounts.pool_token_fees_vault_key,
                &pool_key,
                &accounts.token_a_vault_key,
                &accounts.token_b_vault_key,
                &token_a_key,
                &token_b_key,
                &accounts.token_a_mint_key,
                &accounts.token_b_mint_key,
                &accounts.swap_curve_key,
                ix::WithdrawAllTokenTypes {
                    pool_token_amount,
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                },
            )
            .unwrap(),
            vec![
                &mut accounts.pool_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
                &mut accounts.pool_token_mint_account,
                &mut pool_account,
                &mut accounts.token_a_vault_account,
                &mut accounts.token_b_vault_account,
                &mut token_a_account,
                &mut token_b_account,
                &mut accounts.pool_token_fees_vault_account,
                &mut accounts.token_a_mint_account,
                &mut accounts.token_b_mint_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
                &mut accounts.swap_curve_account,
            ],
        )
        .unwrap();
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_withdraw_one_exact_out_with_invalid_fee_account(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        let user_key = Pubkey::new_unique();

        let fees = Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 2,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 10,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 5,
            host_fee_numerator: 7,
            host_fee_denominator: 100,
        };

        let token_a_amount = 1000;
        let token_b_amount = 2000;
        let curve_params = CurveParameters::ConstantProduct;
        let swap_curve = SwapCurve::new_from_params(curve_params.clone());

        let withdrawer_key = Pubkey::new_unique();
        let initial_a = token_a_amount / 10;
        let initial_b = token_b_amount / 10;
        let initial_pool = swap_curve.calculator.new_pool_supply() / 10;
        let maximum_pool_token_amount = to_u64(initial_pool / 4).unwrap();
        let destination_a_amount = initial_a / 40;

        let mut accounts = SwapAccountInfo::new(
            &user_key,
            fees,
            SwapTransferFees::default(),
            curve_params,
            InitialSupply {
                initial_supply_a: token_a_amount,
                initial_supply_b: token_b_amount,
            },
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );

        accounts.initialize_pool().unwrap();

        let (
            token_a_key,
            mut token_a_account,
            _token_b_key,
            _token_b_account,
            pool_key,
            mut pool_account,
        ) = accounts.setup_token_accounts(
            &user_key,
            &withdrawer_key,
            initial_a,
            initial_b,
            initial_pool.try_into().unwrap(),
        );

        let destination_key = Pubkey::new_unique();
        let mut destination = SolanaAccount::new(
            account_minimum_balance(),
            Account::get_packed_len(),
            &withdrawer_key,
        );

        do_process_instruction(
            close_account(
                &accounts.pool_token_program_id,
                &accounts.pool_token_fees_vault_key,
                &destination_key,
                &user_key,
                &[],
            )
            .unwrap(),
            vec![
                &mut accounts.pool_token_fees_vault_account,
                &mut destination,
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        let user_transfer_authority_key = Pubkey::new_unique();

        do_process_instruction(
            approve(
                &accounts.pool_token_program_id,
                &pool_key,
                &user_transfer_authority_key,
                &withdrawer_key,
                &[],
                maximum_pool_token_amount,
            )
            .unwrap(),
            vec![
                &mut pool_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        do_process_instruction(
            ix::withdraw_single_token_type_exact_amount_out(
                &crate::id(),
                &accounts.pool_token_program_id,
                &token_a_program_id,
                &accounts.pool,
                &accounts.pool_authority,
                &user_transfer_authority_key,
                &accounts.pool_token_mint_key,
                &accounts.pool_token_fees_vault_key,
                &pool_key,
                &accounts.token_a_vault_key,
                &accounts.token_b_vault_key,
                &token_a_key,
                &accounts.token_a_mint_key,
                &accounts.swap_curve_key,
                ix::WithdrawSingleTokenTypeExactAmountOut {
                    destination_token_amount: destination_a_amount,
                    maximum_pool_token_amount,
                },
            )
            .unwrap(),
            vec![
                &mut accounts.pool_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
                &mut accounts.pool_token_mint_account,
                &mut pool_account,
                &mut accounts.token_a_vault_account,
                &mut accounts.token_b_vault_account,
                &mut token_a_account,
                &mut accounts.pool_token_fees_vault_account,
                &mut accounts.token_a_mint_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
                &mut accounts.swap_curve_account,
            ],
        )
        .unwrap();
    }

    #[test_case(spl_token::id(), spl_token::id(), spl_token::id(); "all-token")]
    #[test_case(spl_token::id(), spl_token_2022::id(), spl_token_2022::id(); "mixed-pool-token")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token_2022::id(); "all-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token_2022::id(), spl_token::id(); "a-only-token-2022")]
    #[test_case(spl_token_2022::id(), spl_token::id(), spl_token_2022::id(); "b-only-token-2022")]
    fn test_swap_curve_with_transfer_fees(
        pool_token_program_id: Pubkey,
        token_a_program_id: Pubkey,
        token_b_program_id: Pubkey,
    ) {
        // All fees
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 10;
        let owner_trade_fee_numerator = 1;
        let owner_trade_fee_denominator = 30;
        let owner_withdraw_fee_numerator = 1;
        let owner_withdraw_fee_denominator = 30;
        let host_fee_numerator = 20;
        let host_fee_denominator = 100;
        let fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };

        let token_a_amount = 10_000_000_000;
        let token_b_amount = 50_000_000_000;

        check_valid_swap_curve(
            fees,
            SwapTransferFees {
                _pool_token: TransferFee::default(),
                token_a: TransferFee {
                    epoch: 0.into(),
                    transfer_fee_basis_points: 100.into(),
                    maximum_fee: 1_000_000_000.into(),
                },
                token_b: TransferFee::default(),
            },
            CurveParameters::ConstantProduct,
            token_a_amount,
            token_b_amount,
            &pool_token_program_id,
            &token_a_program_id,
            &token_b_program_id,
        );
    }
}
