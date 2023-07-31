use anchor_lang::{error::ErrorCode as AnchorError, prelude::*};
use anchor_spl::{
    token::spl_token,
    token_2022::{
        spl_token_2022,
        spl_token_2022::{
            error::TokenError,
            extension::{transfer_fee::TransferFee, StateWithExtensions},
            state::Account,
        },
    },
};
use solana_sdk::account::{create_account_for_test, Account as SolanaAccount, WritableAccount};
use test_case::test_case;

use crate::{
    constraints::SwapConstraints,
    curve::{base::CurveType, fees::Fees},
    error::SwapError,
    instructions::test::runner::{
        processor::{
            do_process_instruction, do_process_instruction_with_fee_constraints, SwapAccountInfo,
            SwapTransferFees,
        },
        token,
    },
    ix,
    ix::Initialize,
    model::CurveParameters,
    InitialSupply,
};

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

    assert::check_valid_swap_curve(
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
    assert::check_valid_swap_curve(
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
    assert::check_valid_swap_curve(
        fees,
        SwapTransferFees::default(),
        CurveParameters::Offset { token_b_offset },
        token_a_amount,
        token_b_amount,
        &pool_token_program_id,
        &token_a_program_id,
        &token_b_program_id,
    );
    let amp = 100;
    let token_a_decimals = 6;
    let token_b_decimals = 6;
    assert::check_valid_swap_curve(
        fees,
        SwapTransferFees::default(),
        CurveParameters::Stable {
            amp,
            token_a_decimals,
            token_b_decimals,
        },
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

    assert::check_valid_swap_curve(
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
    assert::check_valid_swap_curve(
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
    assert::check_valid_swap_curve(
        fees,
        SwapTransferFees::default(),
        CurveParameters::Offset { token_b_offset },
        token_a_amount,
        token_b_amount,
        &pool_token_program_id,
        &token_a_program_id,
        &token_b_program_id,
    );
    let amp = 100;
    let token_a_decimals = 6;
    let token_b_decimals = 6;
    assert::check_valid_swap_curve(
        fees,
        SwapTransferFees::default(),
        CurveParameters::Stable {
            amp,
            token_a_decimals,
            token_b_decimals,
        },
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
        blocked_trading_token_extensions: &[],
    });
    let mut accounts = SwapAccountInfo::new(
        &owner_key,
        fees,
        SwapTransferFees::default(),
        curve_params,
        InitialSupply::new(token_a_amount, token_b_amount),
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
            &accounts.token_a_fees_vault_key,
            &accounts.token_b_fees_vault_key,
            &accounts.admin_authority_token_a_ata_key,
            &accounts.admin_authority_token_b_ata_key,
            &accounts.admin_authority_pool_token_ata_key,
            &accounts.pool_token_program_id,
            &accounts.token_a_program_id,
            &accounts.token_b_program_id,
            Initialize {
                fees: accounts.fees,
                initial_supply: accounts.initial_supply.clone(),
                curve_parameters: accounts.curve_params.clone().into(),
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
            &mut accounts.token_a_fees_vault_account,
            &mut accounts.token_b_fees_vault_account,
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
        user_token_a_key,
        mut token_a_account,
        user_token_b_key,
        mut token_b_account,
        _pool_key,
        _pool_account,
    ) = accounts.setup_token_accounts(
        &owner_key,
        &authority_key,
        token_a_amount,
        token_b_amount,
        0,
    );

    let (
        host_fee_token_a_key,
        mut host_fee_a_account,
        _host_fee_token_b_key,
        _host_fee_token_b_account,
        _,
        _,
    ) = accounts.setup_token_accounts(&owner_key, &authority_key, 0, 0, 0);

    let amount_in = token_a_amount / 2;
    let minimum_amount_out = 0;

    let exe = &mut SolanaAccount::default();
    exe.set_executable(true);

    // perform the swap
    do_process_instruction_with_fee_constraints(
        ix::swap(
            &crate::id(),
            &authority_key,
            &accounts.pool,
            &accounts.swap_curve_key,
            &accounts.pool_authority,
            &accounts.token_a_mint_key,
            &accounts.token_b_mint_key,
            &accounts.token_a_vault_key,
            &accounts.token_b_vault_key,
            &accounts.token_a_fees_vault_key,
            &user_token_a_key,
            &user_token_b_key,
            Some(&host_fee_token_a_key),
            &token_a_program_id,
            &token_b_program_id,
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
            &mut accounts.token_a_fees_vault_account,
            &mut token_a_account,
            &mut token_b_account,
            &mut host_fee_a_account,
            &mut exe.clone(), // source_token_program
            &mut exe.clone(), // destination_token_program
        ],
        &constraints,
    )
    .unwrap();

    // check that fees were taken in the host fee account
    let host_token_a_fees_vault =
        StateWithExtensions::<Account>::unpack(&host_fee_a_account.data).unwrap();
    let owner_token_a_fees_vault =
        StateWithExtensions::<Account>::unpack(&accounts.token_a_fees_vault_account.data).unwrap();
    let total_fee = owner_token_a_fees_vault.base.amount * host_fee_denominator
        / (host_fee_denominator - host_fee_numerator);
    assert_eq!(
        total_fee,
        host_token_a_fees_vault.base.amount + owner_token_a_fees_vault.base.amount
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
        InitialSupply::new(token_a_amount, token_b_amount),
        &pool_token_program_id,
        &token_a_program_id,
        &token_b_program_id,
    );

    let initial_a = token_a_amount / 5;
    let initial_b = token_b_amount / 5;
    let minimum_token_b_amount = initial_b / 2;

    let token_a_vault_key = accounts.token_a_vault_key;
    let token_b_vault_key = accounts.token_b_vault_key;
    let token_a_fees_vault_key = accounts.token_a_fees_vault_key;
    let token_b_fees_vault_key = accounts.token_b_fees_vault_key;

    // swap not initialized
    {
        let (token_a_key, mut token_a_account) = token::create_token_account(
            &accounts.token_a_program_id,
            &accounts.token_a_mint_key,
            &mut accounts.token_a_mint_account,
            &user_key,
            &swapper_key,
            initial_a,
        );
        let (token_b_key, mut token_b_account) = token::create_token_account(
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
                &token_a_vault_key,
                &token_a_fees_vault_key,
                &token_b_vault_key,
                &token_b_key,
                &mut token_b_account,
                initial_a,
                minimum_token_b_amount,
            )
        );
    }

    accounts.initialize_pool().unwrap();

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
                &token_a_vault_key,
                &token_a_fees_vault_key,
                &token_b_vault_key,
                &token_b_key,
                &mut token_b_account,
                initial_a * 2,
                minimum_token_b_amount * 2,
            )
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
            Err(SwapError::RepeatedMint.into()),
            accounts.swap(
                &swapper_key,
                &token_a_key,
                &mut token_a_account.clone(),
                &token_a_vault_key,
                &token_a_fees_vault_key,
                &token_a_vault_key,
                &token_a_key,
                &mut token_a_account,
                initial_a,
                minimum_token_b_amount,
            )
        );
    }

    // no approval
    {
        let (
            user_token_a_key,
            mut token_a_account,
            user_token_b_key,
            mut token_b_account,
            _pool_key,
            _pool_account,
        ) = accounts.setup_token_accounts(&user_key, &swapper_key, initial_a, initial_b, 0);
        let user_transfer_key = Pubkey::new_unique();

        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);

        assert_eq!(
            Err(TokenError::OwnerMismatch.into()),
            do_process_instruction(
                ix::swap(
                    &crate::id(),
                    &user_transfer_key,
                    &accounts.pool,
                    &accounts.swap_curve_key,
                    &accounts.pool_authority,
                    &accounts.token_a_mint_key,
                    &accounts.token_b_mint_key,
                    &accounts.token_a_vault_key,
                    &accounts.token_b_vault_key,
                    &accounts.token_a_fees_vault_key,
                    &user_token_a_key,
                    &user_token_b_key,
                    None,
                    &token_a_program_id,
                    &token_b_program_id,
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
                    &mut accounts.token_a_fees_vault_account,
                    &mut token_a_account,
                    &mut token_b_account,
                    &mut exe.clone(), // Optional front end host fees - passed as the program if not present
                    &mut exe.clone(), // source_token_program
                    &mut exe.clone(), // destination_token_program
                ],
            ),
        );
    }

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
            Err(SwapError::ZeroTradingTokens.into()),
            accounts.swap(
                &swapper_key,
                &token_b_key,
                &mut token_b_account,
                &token_b_vault_key,
                &token_b_fees_vault_key,
                &token_a_vault_key,
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
            Err(SwapError::ExceededSlippage.into()),
            accounts.swap(
                &swapper_key,
                &token_a_key,
                &mut token_a_account,
                &token_a_vault_key,
                &token_a_fees_vault_key,
                &token_b_vault_key,
                &token_b_key,
                &mut token_b_account,
                initial_a,
                minimum_token_b_amount * 2,
            )
        );
    }

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
            blocked_trading_token_extensions: &[],
        });

        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);

        do_process_instruction_with_fee_constraints(
            ix::swap(
                &crate::id(),
                &accounts.pool_authority,
                &accounts.pool,
                &accounts.swap_curve_key,
                &accounts.pool_authority,
                &accounts.token_a_mint_key,
                &accounts.token_b_mint_key,
                &accounts.token_a_vault_key,
                &accounts.token_b_vault_key,
                &accounts.token_a_fees_vault_key,
                &token_a_key,
                &token_b_key,
                None,
                &token_a_program_id,
                &token_b_program_id,
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
                &mut accounts.token_a_fees_vault_account,
                &mut token_a_account,
                &mut token_b_account,
                &mut exe.clone(), // Optional front end host fees - passed as the program if not present
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
            _token_a_key,
            mut _token_a_account,
            bad_token_a_key,
            mut bad_token_a_account,
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
            blocked_trading_token_extensions: &[],
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
                    &accounts.pool_authority,
                    &accounts.pool,
                    &accounts.swap_curve_key,
                    &accounts.pool_authority,
                    &accounts.token_a_mint_key,
                    &accounts.token_b_mint_key,
                    &accounts.token_a_vault_key,
                    &accounts.token_b_vault_key,
                    &accounts.token_a_fees_vault_key,
                    &token_a_key,
                    &token_b_key,
                    Some(&bad_token_a_key),
                    &token_a_program_id,
                    &token_b_program_id,
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
                    &mut accounts.token_a_fees_vault_account,
                    &mut token_a_account,
                    &mut token_b_account,
                    &mut bad_token_a_account, // Optional front end host fees - passed as the program if not present
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
        InitialSupply::new(token_a_amount, token_b_amount),
        &pool_token_program_id,
        &token_a_program_id,
        &token_b_program_id,
    );

    accounts.initialize_pool().unwrap();

    let token_a_vault_key = accounts.token_a_vault_key;
    let token_b_vault_key = accounts.token_b_vault_key;
    let token_a_fees_vault_key = accounts.token_a_fees_vault_key;
    let token_b_fees_vault_key = accounts.token_b_fees_vault_key;
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
        Err(SwapError::ZeroTradingTokens.into()),
        accounts.swap(
            &swapper_key,
            &token_a_key,
            &mut token_a_account,
            &token_a_vault_key,
            &token_a_fees_vault_key,
            &token_b_vault_key,
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
            &token_b_vault_key,
            &token_b_fees_vault_key,
            &token_a_vault_key,
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
            &token_a_vault_key,
            &token_a_fees_vault_key,
            &token_b_vault_key,
            &token_b_key,
            &mut token_b_account,
            a_to_b_amount,
            minimum_token_b_amount,
        )
        .unwrap();

    // try a to b again, fails due to no more liquidity
    assert_eq!(
        Err(SwapError::ZeroTradingTokens.into()),
        accounts.swap(
            &swapper_key,
            &token_a_key,
            &mut token_a_account,
            &token_a_vault_key,
            &token_a_fees_vault_key,
            &token_b_vault_key,
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
            Err(SwapError::UnsupportedCurveOperation.into()),
            accounts.deposit(
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

    assert::check_valid_swap_curve(
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

mod assert {
    use crate::curve::base::SwapFeeInputs;
    use super::*;
    use crate::curve::calculator::TradeDirection;

    #[allow(clippy::too_many_arguments)]
    pub fn check_valid_swap_curve(
        pool_fees: Fees,
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
            pool_fees,
            transfer_fees,
            curve_params,
            InitialSupply::new(token_a_amount, token_b_amount),
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

        let token_a_vault_key = accounts.token_a_vault_key;
        let token_b_vault_key = accounts.token_b_vault_key;
        let token_a_fees_vault_key = accounts.token_a_fees_vault_key;
        let token_b_fees_vault_key = accounts.token_b_fees_vault_key;

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

        accounts
            .swap(
                &swapper_key,
                &token_a_key,
                &mut token_a_account,
                &token_a_vault_key,
                &token_a_fees_vault_key,
                &token_b_vault_key,
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
                &SwapFeeInputs::pool_fees(&pool_fees),
            )
            .unwrap();

        let swap_token_a =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_vault_account.data).unwrap();
        let token_a_amount = swap_token_a.base.amount;
        assert_eq!(
            token_a_amount,
            TryInto::<u64>::try_into(results.new_pool_source_amount).unwrap()
        );
        let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
        assert_eq!(token_a.base.amount, initial_a - a_to_b_amount);

        let swap_token_b =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data).unwrap();
        let token_b_amount = swap_token_b.base.amount;
        assert_eq!(
            token_b_amount,
            TryInto::<u64>::try_into(results.new_pool_destination_amount).unwrap()
        );
        let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
        assert_eq!(
            token_b.base.amount,
            initial_b + u64::try_from(results.destination_amount_swapped).unwrap()
        );

        let first_fee = TryInto::<u64>::try_into(results.owner_fee).unwrap();
        let token_a_fee_account =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_fees_vault_account.data)
                .unwrap();
        assert_eq!(token_a_fee_account.base.amount, first_fee);
        let token_b_fee_account =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_fees_vault_account.data)
                .unwrap();
        assert_eq!(token_b_fee_account.base.amount, 0);

        let first_swap_amount = results.destination_amount_swapped;

        // swap the other way
        let b_to_a_amount = initial_b / 10;
        let minimum_a_amount = 0;
        accounts
            .swap(
                &swapper_key,
                &token_b_key,
                &mut token_b_account,
                &token_b_vault_key,
                &token_b_fees_vault_key,
                &token_a_vault_key,
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
                &SwapFeeInputs::pool_fees(&pool_fees),
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
            TryInto::<u64>::try_into(results.new_pool_destination_amount).unwrap()
        );
        let token_a = StateWithExtensions::<Account>::unpack(&token_a_account.data).unwrap();
        assert_eq!(
            token_a.base.amount,
            initial_a - a_to_b_amount + u64::try_from(results.destination_amount_swapped).unwrap()
        );

        let swap_token_b =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_vault_account.data).unwrap();
        let token_b_amount = swap_token_b.base.amount;
        assert_eq!(
            token_b_amount,
            TryInto::<u64>::try_into(results.new_pool_source_amount).unwrap()
        );
        let token_b = StateWithExtensions::<Account>::unpack(&token_b_account.data).unwrap();
        // assert_eq!(
        //     token_b.base.amount,
        //     initial_b + u64::try_from(first_swap_amount).unwrap()
        //         - u64::try_from(results.total_source_amount_swapped).unwrap()
        // );

        // todo elliot
        let second_fee = TryInto::<u64>::try_into(results.owner_fee).unwrap();
        let token_a_fee_account =
            StateWithExtensions::<Account>::unpack(&accounts.token_a_fees_vault_account.data)
                .unwrap();
        assert_eq!(token_a_fee_account.base.amount, first_fee);
        let token_b_fee_account =
            StateWithExtensions::<Account>::unpack(&accounts.token_b_fees_vault_account.data)
                .unwrap();
        assert_eq!(token_b_fee_account.base.amount, second_fee);
    }
}
