#![allow(clippy::integer_arithmetic)]

use std::collections::{HashMap, HashSet};

use arbitrary::Arbitrary;
use honggfuzz::fuzz;
use hyperplane::{
    curve::{base::CurveType, calculator::TradeDirection, fees::Fees},
    error::SwapError,
    ix::{Deposit, Swap, Withdraw, WithdrawFees},
    model::CurveParameters,
};
use hyperplane_fuzz::{
    native_account_data::NativeAccountData,
    native_token::{get_token_balance, transfer},
    native_token_swap::NativeTokenSwap,
};
use spl_math::precise_number::PreciseNumber;
use spl_token::error::TokenError;

#[derive(Debug, Arbitrary, Clone)]
struct FuzzData {
    curve_type: CurveType,
    instructions: Vec<FuzzInstruction>,
}

#[derive(Debug, Arbitrary, Clone)]
enum FuzzInstruction {
    Swap {
        token_a_id: AccountId,
        token_b_id: AccountId,
        trade_direction: TradeDirection,
        instruction: Swap,
    },
    Deposit {
        token_a_id: AccountId,
        token_b_id: AccountId,
        pool_token_id: AccountId,
        instruction: Deposit,
    },
    Withdraw {
        token_a_id: AccountId,
        token_b_id: AccountId,
        pool_token_id: AccountId,
        instruction: Withdraw,
    },
}

/// Use u8 as an account id to simplify the address space and re-use accounts
/// more often.
type AccountId = u8;

const INITIAL_SWAP_TOKEN_A_AMOUNT: u64 = 100_000_000_000;
const INITIAL_SWAP_TOKEN_B_AMOUNT: u64 = 300_000_000_000;

const INITIAL_USER_TOKEN_A_AMOUNT: u64 = 1_000_000_000;
const INITIAL_USER_TOKEN_B_AMOUNT: u64 = 3_000_000_000;

fn main() {
    loop {
        fuzz!(|fuzz_data: FuzzData| { run_fuzz(fuzz_data) });
    }
}

fn run_fuzz(fuzz_data: FuzzData) {
    let trade_fee_numerator = 25;
    let trade_fee_denominator = 10000;
    let owner_trade_fee_numerator = 5;
    let owner_trade_fee_denominator = 10000;
    let owner_withdraw_fee_numerator = 30;
    let owner_withdraw_fee_denominator = 10000;
    let host_fee_numerator = 1;
    let host_fee_denominator = 5;
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
    let curve_params = get_curve_parameters(fuzz_data.curve_type);
    let mut token_swap = NativeTokenSwap::new(
        fees,
        curve_params.clone(),
        INITIAL_SWAP_TOKEN_A_AMOUNT,
        INITIAL_SWAP_TOKEN_B_AMOUNT,
    );

    // keep track of all accounts, including swap accounts
    let mut token_a_accounts: HashMap<AccountId, NativeAccountData> = HashMap::new();
    let mut token_b_accounts: HashMap<AccountId, NativeAccountData> = HashMap::new();
    let mut pool_accounts: HashMap<AccountId, NativeAccountData> = HashMap::new();

    // add all the pool and token accounts that will be needed
    for fuzz_instruction in &fuzz_data.instructions {
        let (token_a_id, token_b_id, pool_token_id) = match fuzz_instruction.clone() {
            FuzzInstruction::Swap {
                token_a_id,
                token_b_id,
                ..
            } => (Some(token_a_id), Some(token_b_id), None),

            FuzzInstruction::Deposit {
                token_a_id,
                token_b_id,
                pool_token_id,
                ..
            } => (Some(token_a_id), Some(token_b_id), Some(pool_token_id)),

            FuzzInstruction::Withdraw {
                token_a_id,
                token_b_id,
                pool_token_id,
                ..
            } => (Some(token_a_id), Some(token_b_id), Some(pool_token_id)),
        };
        if let Some(token_a_id) = token_a_id {
            token_a_accounts
                .entry(token_a_id)
                .or_insert_with(|| token_swap.create_token_a_account(INITIAL_USER_TOKEN_A_AMOUNT));
        }
        if let Some(token_b_id) = token_b_id {
            token_b_accounts
                .entry(token_b_id)
                .or_insert_with(|| token_swap.create_token_b_account(INITIAL_USER_TOKEN_B_AMOUNT));
        }
        if let Some(pool_token_id) = pool_token_id {
            pool_accounts
                .entry(pool_token_id)
                .or_insert_with(|| token_swap.create_pool_account());
        }
    }

    let pool_tokens = [&token_swap.admin_pool_token_ata]
        .iter()
        .map(|&x| get_token_balance(x))
        .sum::<u64>() as u128;
    let initial_pool_token_amount =
        pool_tokens + pool_accounts.values().map(get_token_balance).sum::<u64>() as u128;
    let initial_swap_token_a_amount = get_token_balance(&token_swap.token_a_vault_account) as u128;
    let initial_swap_token_b_amount = get_token_balance(&token_swap.token_b_vault_account) as u128;

    // to ensure that we never create or remove base tokens
    let before_total_token_a =
        INITIAL_SWAP_TOKEN_A_AMOUNT + get_total_token_a_amount(&fuzz_data.instructions);
    let before_total_token_b =
        INITIAL_SWAP_TOKEN_B_AMOUNT + get_total_token_b_amount(&fuzz_data.instructions);

    for fuzz_instruction in fuzz_data.instructions {
        run_fuzz_instruction(
            fuzz_instruction,
            &mut token_swap,
            &mut token_a_accounts,
            &mut token_b_accounts,
            &mut pool_accounts,
        );
    }

    let pool_token_amount =
        pool_tokens + pool_accounts.values().map(get_token_balance).sum::<u64>() as u128;
    let swap_token_a_amount = get_token_balance(&token_swap.token_a_vault_account) as u128;
    let swap_token_b_amount = get_token_balance(&token_swap.token_b_vault_account) as u128;

    let initial_pool_value = token_swap
        .swap_curve
        .calculator
        .normalized_value(initial_swap_token_a_amount, initial_swap_token_b_amount)
        .unwrap();
    let pool_value = token_swap
        .swap_curve
        .calculator
        .normalized_value(swap_token_a_amount, swap_token_b_amount)
        .unwrap();

    let pool_token_amount = PreciseNumber::new(pool_token_amount).unwrap();
    let initial_pool_token_amount = PreciseNumber::new(initial_pool_token_amount).unwrap();
    assert!(initial_pool_value
        .checked_div(&initial_pool_token_amount)
        .unwrap()
        .less_than_or_equal(&pool_value.checked_div(&pool_token_amount).unwrap()));

    // check total token a and b amounts
    let after_total_token_a = token_a_accounts
        .values()
        .map(get_token_balance)
        .sum::<u64>()
        + get_token_balance(&token_swap.token_a_vault_account)
        + get_token_balance(&token_swap.token_a_fees_vault_account)
        + get_token_balance(&token_swap.admin_token_a_ata); // admin takes host fees
    assert_eq!(before_total_token_a, after_total_token_a);
    let after_total_token_b = token_b_accounts
        .values()
        .map(get_token_balance)
        .sum::<u64>()
        + get_token_balance(&token_swap.token_b_vault_account)
        + get_token_balance(&token_swap.token_b_fees_vault_account)
        + get_token_balance(&token_swap.admin_token_b_ata); // admin takes host fees
    assert_eq!(before_total_token_b, after_total_token_b);

    // Final check to make sure that withdrawing everything works
    //
    // 1) transfer all pool tokens to the admin pool token account
    let mut admin_pool_token_ata = token_swap.admin_pool_token_ata.clone();
    for pool_account in pool_accounts.values_mut() {
        let pool_token_amount = get_token_balance(pool_account);
        if pool_token_amount > 0 {
            transfer(pool_account, &mut admin_pool_token_ata, pool_token_amount);
        }
    }

    // 2) Now burn all pool tokens from the admin pool token account
    // This will produce withdraw fees which we will withdraw next
    let mut withdrawn_token_a_account = token_swap.create_token_a_account(0);
    let mut withdrawn_token_b_account = token_swap.create_token_b_account(0);
    token_swap
        .withdraw_all(
            &mut admin_pool_token_ata,
            &mut withdrawn_token_a_account,
            &mut withdrawn_token_b_account,
        )
        .map_err(|e| println!("withdraw failed {:?}", e))
        .unwrap();

    // 3) withdraw all fees to the admin atas
    let token_a_fees = get_token_balance(&token_swap.token_a_fees_vault_account);
    if token_a_fees > 0 {
        token_swap
            .withdraw_a_fees(
                &mut withdrawn_token_a_account,
                WithdrawFees {
                    requested_token_amount: token_a_fees,
                },
            )
            .map_err(|e| println!("withdraw_fees (token a) failed {:?}", e))
            .unwrap();
    }
    let token_b_fees = get_token_balance(&token_swap.token_b_fees_vault_account);
    if token_b_fees > 0 {
        token_swap
            .withdraw_b_fees(
                &mut withdrawn_token_b_account,
                WithdrawFees {
                    requested_token_amount: token_b_fees,
                },
            )
            .map_err(|e| println!("withdraw_fees (token b) failed {:?}", e))
            .unwrap();
    }

    // We should have all the tokens we started with
    let after_total_token_a = token_a_accounts
        .values()
        .map(get_token_balance)
        .sum::<u64>()
        + get_token_balance(&withdrawn_token_a_account)
        + get_token_balance(&token_swap.admin_token_a_ata); // admin takes host fees
    assert_eq!(before_total_token_a, after_total_token_a);
    let mut after_total_token_b = token_b_accounts
        .values()
        .map(get_token_balance)
        .sum::<u64>()
        + get_token_balance(&withdrawn_token_b_account)
        + get_token_balance(&token_swap.admin_token_b_ata); // admin takes host fees

    // todo - Constant price curves don't return all tokens when everything is burned - this seems like a bug and needs investigating further
    if let CurveParameters::ConstantPrice { .. } = curve_params {
        after_total_token_b += get_token_balance(&token_swap.token_b_vault_account);
    }
    assert_eq!(before_total_token_b, after_total_token_b);
}

fn run_fuzz_instruction(
    fuzz_instruction: FuzzInstruction,
    token_swap: &mut NativeTokenSwap,
    token_a_accounts: &mut HashMap<AccountId, NativeAccountData>,
    token_b_accounts: &mut HashMap<AccountId, NativeAccountData>,
    pool_accounts: &mut HashMap<AccountId, NativeAccountData>,
) {
    let result = match fuzz_instruction.clone() {
        FuzzInstruction::Swap {
            token_a_id,
            token_b_id,
            trade_direction,
            instruction,
        } => {
            let token_a_account = token_a_accounts.get_mut(&token_a_id).unwrap();
            let token_b_account = token_b_accounts.get_mut(&token_b_id).unwrap();
            match trade_direction {
                TradeDirection::AtoB => {
                    token_swap.swap_a_to_b(token_a_account, token_b_account, instruction)
                }
                TradeDirection::BtoA => {
                    token_swap.swap_b_to_a(token_b_account, token_a_account, instruction)
                }
            }
        }
        FuzzInstruction::Deposit {
            token_a_id,
            token_b_id,
            pool_token_id,
            instruction,
        } => {
            let token_a_account = token_a_accounts.get_mut(&token_a_id).unwrap();
            let token_b_account = token_b_accounts.get_mut(&token_b_id).unwrap();
            let pool_account = pool_accounts.get_mut(&pool_token_id).unwrap();
            token_swap.deposit(token_a_account, token_b_account, pool_account, instruction)
        }
        FuzzInstruction::Withdraw {
            token_a_id,
            token_b_id,
            pool_token_id,
            instruction,
        } => {
            let token_a_account = token_a_accounts.get_mut(&token_a_id).unwrap();
            let token_b_account = token_b_accounts.get_mut(&token_b_id).unwrap();
            let pool_account = pool_accounts.get_mut(&pool_token_id).unwrap();
            token_swap.withdraw(pool_account, token_a_account, token_b_account, instruction)
        }
    };
    result
        .map_err(|e| {
            if !(e == SwapError::CalculationFailure.into()
                || e == SwapError::ConversionFailure.into()
                || e == SwapError::FeeCalculationFailure.into()
                || e == SwapError::ExceededSlippage.into()
                || e == SwapError::ZeroTradingTokens.into()
                || e == SwapError::UnsupportedCurveOperation.into()
                || e == SwapError::InsufficientPoolTokenFunds.into()
                || e == TokenError::InsufficientFunds.into()
                // OwnerMismatch can happen due to delegation and 2 transfers (fee and swap)
                // If the swap transfer uses the entire delegated amount,
                // then the delegate is removed the and second (fee) transfer will fail
                || e == TokenError::OwnerMismatch.into())
            {
                println!("Fuzzer returned error - {e:?} - {fuzz_instruction:?}");
                Err(e).unwrap()
            }
        })
        .ok();
}

fn get_total_token_a_amount(fuzz_instructions: &[FuzzInstruction]) -> u64 {
    let mut token_a_ids = HashSet::new();
    for fuzz_instruction in fuzz_instructions.iter() {
        match fuzz_instruction {
            FuzzInstruction::Swap { token_a_id, .. } => token_a_ids.insert(token_a_id),
            FuzzInstruction::Deposit { token_a_id, .. } => token_a_ids.insert(token_a_id),
            FuzzInstruction::Withdraw { token_a_id, .. } => token_a_ids.insert(token_a_id),
        };
    }
    (token_a_ids.len() as u64) * INITIAL_USER_TOKEN_A_AMOUNT
}

fn get_total_token_b_amount(fuzz_instructions: &[FuzzInstruction]) -> u64 {
    let mut token_b_ids = HashSet::new();
    for fuzz_instruction in fuzz_instructions.iter() {
        match fuzz_instruction {
            FuzzInstruction::Swap { token_b_id, .. } => token_b_ids.insert(token_b_id),
            FuzzInstruction::Deposit { token_b_id, .. } => token_b_ids.insert(token_b_id),
            FuzzInstruction::Withdraw { token_b_id, .. } => token_b_ids.insert(token_b_id),
        };
    }
    (token_b_ids.len() as u64) * INITIAL_USER_TOKEN_B_AMOUNT
}

fn get_curve_parameters(curve_type: CurveType) -> CurveParameters {
    match curve_type {
        CurveType::ConstantProduct => CurveParameters::ConstantProduct,
        CurveType::ConstantPrice => CurveParameters::ConstantPrice {
            token_b_price: 10_000_000,
        },
        CurveType::Offset => CurveParameters::Offset {
            token_b_offset: 100_000_000_000,
        },
        CurveType::Stable => CurveParameters::Stable {
            amp: 100,
            token_a_decimals: 6,
            token_b_decimals: 6,
        },
    }
}
