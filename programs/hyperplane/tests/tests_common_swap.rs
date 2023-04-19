// useful for d.p. clarity in tests
#![allow(clippy::inconsistent_digit_grouping)]

mod common;

use common::{client, runner};
use hyperplane::{
    curve::{calculator::TradeDirection, fees::Fees},
    error::SwapError,
    ix::{Swap, UpdatePoolConfig},
    state::{SwapState, UpdatePoolConfigMode, UpdatePoolConfigValue},
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};

use crate::common::{
    fixtures, setup, setup::default_supply, state, token_operations, types::SwapPairSpec,
};

#[tokio::test]
pub async fn test_swap_fails_with_withdrawal_only_mode() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        default_supply(),
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    client::update_pool_config(
        &mut ctx,
        &pool,
        UpdatePoolConfig::new(
            UpdatePoolConfigMode::WithdrawalsOnly,
            UpdatePoolConfigValue::Bool(true),
        ),
    )
    .await
    .unwrap();
    let pool_state = state::get_pool(&mut ctx, &pool).await;
    assert!(pool_state.withdrawals_only());

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
    assert_eq!(
        client::swap(
            &mut ctx,
            &pool,
            &user,
            TradeDirection::AtoB,
            Swap {
                amount_in: 50,
                minimum_amount_out: 47,
            },
        )
        .await
        .unwrap_err()
        .unwrap(),
        hyperplane_error!(SwapError::WithdrawalsOnlyMode)
    );

    // unset withdrawals_only mode
    client::update_pool_config(
        &mut ctx,
        &pool,
        UpdatePoolConfig::new(
            UpdatePoolConfigMode::WithdrawalsOnly,
            UpdatePoolConfigValue::Bool(false),
        ),
    )
    .await
    .unwrap();
    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::AtoB,
        Swap {
            amount_in: 50,
            minimum_amount_out: 47,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
pub async fn test_swap_with_host_fees_less_than_one_rounds_down_to_zero() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            host_fee_numerator: 1,
            host_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
    let host_fees = setup::new_pool_user(&mut ctx, &pool, (0, 0)).await;

    client::swap_with_host_fees(
        &mut ctx,
        &pool,
        &user,
        Some(&host_fees),
        TradeDirection::AtoB,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 53);

    // owner get the 1 fee payed into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 1);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    // no host fees payed host fee account
    let host_token_a_fees_balance =
        token_operations::balance(&mut ctx, &host_fees.token_a_ata).await;
    assert_eq!(host_token_a_fees_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 47);
}
