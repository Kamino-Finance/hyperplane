mod common;

use common::{client, runner};
use hyperplane::{
    curve::{calculator::TradeDirection, fees::Fees},
    error::SwapError,
    ix::{Swap, WithdrawFees},
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};

use crate::common::{
    fixtures, setup, token_operations,
    types::{AorB, TradingTokenSpec},
};

#[tokio::test]
pub async fn test_successful_withdraw_full_balance() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_numerator: 1,
            host_fee_denominator: 100,
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
        },
        InitialSupply::new(100, 100),
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let initial_admin_fees_balance =
        token_operations::balance(&mut ctx, &pool.admin.token_a_ata).await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::AtoB,
        Swap::new(50, 47),
    )
    .await
    .unwrap();

    let fees_from_swap = token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    client::withdraw_fees(&mut ctx, &pool, AorB::A, WithdrawFees::new(fees_from_swap))
        .await
        .unwrap();

    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    let admin_balance = token_operations::balance(&mut ctx, &pool.admin.token_a_ata).await;
    assert_eq!(fee_vault_balance, 0);
    assert_eq!(admin_balance, fees_from_swap + initial_admin_fees_balance);
}

#[tokio::test]
pub async fn test_successful_withdraw_full_balance_token_b() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_numerator: 1,
            host_fee_denominator: 100,
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
        },
        InitialSupply::new(100, 100),
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let initial_admin_fees_balance =
        token_operations::balance(&mut ctx, &pool.admin.token_b_ata).await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;
    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap::new(50, 47),
    )
    .await
    .unwrap();

    let fees_from_swap = token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    client::withdraw_fees(&mut ctx, &pool, AorB::B, WithdrawFees::new(fees_from_swap))
        .await
        .unwrap();

    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    let admin_balance = token_operations::balance(&mut ctx, &pool.admin.token_b_ata).await;
    assert_eq!(fee_vault_balance, 0);
    assert_eq!(admin_balance, fees_from_swap + initial_admin_fees_balance);
}

#[tokio::test]
pub async fn test_successful_withdraw_full_balance_request_u64_max() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_numerator: 1,
            host_fee_denominator: 100,
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
        },
        InitialSupply::new(100, 100),
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let initial_admin_fees_balance =
        token_operations::balance(&mut ctx, &pool.admin.token_a_ata).await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::AtoB,
        Swap::new(50, 47),
    )
    .await
    .unwrap();

    let fees_from_swap = token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    client::withdraw_fees(&mut ctx, &pool, AorB::A, WithdrawFees::new(u64::MAX))
        .await
        .unwrap();

    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    let admin_balance = token_operations::balance(&mut ctx, &pool.admin.token_a_ata).await;
    assert_eq!(fee_vault_balance, 0);
    assert_eq!(admin_balance, fees_from_swap + initial_admin_fees_balance);
}

#[tokio::test]
pub async fn test_successful_withdraw_partial_balance() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_numerator: 1,
            host_fee_denominator: 100,
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
        },
        InitialSupply::new(10_000_000, 10_000_000),
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let initial_admin_fees_balance =
        token_operations::balance(&mut ctx, &pool.admin.token_a_ata).await;
    let user = setup::new_pool_user(&mut ctx, &pool, (1_000_000, 0)).await;
    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::AtoB,
        Swap::new(1_000_000, 970_000),
    )
    .await
    .unwrap();

    let fees_from_swap = token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    let half_fees_from_swap = fees_from_swap / 2;
    client::withdraw_fees(
        &mut ctx,
        &pool,
        AorB::A,
        WithdrawFees::new(half_fees_from_swap),
    )
    .await
    .unwrap();

    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    let admin_balance = token_operations::balance(&mut ctx, &pool.admin.token_a_ata).await;
    assert_eq!(fee_vault_balance, fees_from_swap - half_fees_from_swap);
    assert_eq!(
        admin_balance,
        half_fees_from_swap + initial_admin_fees_balance
    );
}

#[tokio::test]
pub async fn test_withdraw_0_fails() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_numerator: 1,
            host_fee_denominator: 100,
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
        },
        InitialSupply::new(100, 100),
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::AtoB,
        Swap::new(50, 47),
    )
    .await
    .unwrap();

    assert_eq!(
        client::withdraw_fees(&mut ctx, &pool, AorB::A, WithdrawFees::new(0))
            .await
            .unwrap_err()
            .unwrap(),
        hyperplane_error!(SwapError::ZeroTradingTokens)
    );
}

#[tokio::test]
pub async fn test_withdraw_when_0_in_vault_fails() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_numerator: 1,
            host_fee_denominator: 100,
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
        },
        InitialSupply::new(100, 100),
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    assert_eq!(
        client::withdraw_fees(&mut ctx, &pool, AorB::A, WithdrawFees::new(10))
            .await
            .unwrap_err()
            .unwrap(),
        hyperplane_error!(SwapError::ZeroTradingTokens)
    );
}
