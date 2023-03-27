mod common;

use common::{client, runner};
use hyperplane::{
    curve::{calculator::TradeDirection, fees::Fees},
    error::SwapError,
    ix::Swap,
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};
use solana_sdk::signer::Signer;

use crate::common::{fixtures, setup, token_operations, types::TradingTokenSpec};

#[tokio::test]
pub async fn test_successful_withdraw_full_balance() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_denominator: 100,
            host_fee_numerator: 1,
            trade_fee_denominator: 100,
            trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
        },
        InitialSupply {
            initial_supply_a: 100,
            initial_supply_b: 100,
        },
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let initial_admin_pool_tokens =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
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

    let fees_from_swap = token_operations::balance(&mut ctx, &pool.pool_token_fees_vault).await;
    client::withdraw_fees(&mut ctx, &pool, fees_from_swap)
        .await
        .unwrap();

    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.pool_token_fees_vault).await;
    let admin_balance =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;
    assert_eq!(fee_vault_balance, 0);
    assert_eq!(admin_balance, fees_from_swap + initial_admin_pool_tokens);
}

#[tokio::test]
pub async fn test_successful_withdraw_full_balance_request_u64_max() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_denominator: 100,
            host_fee_numerator: 1,
            trade_fee_denominator: 100,
            trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
        },
        InitialSupply {
            initial_supply_a: 100,
            initial_supply_b: 100,
        },
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let initial_admin_pool_tokens =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
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

    let fees_from_swap = token_operations::balance(&mut ctx, &pool.pool_token_fees_vault).await;
    client::withdraw_fees(&mut ctx, &pool, u64::MAX)
        .await
        .unwrap();

    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.pool_token_fees_vault).await;
    let admin_balance =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;
    assert_eq!(fee_vault_balance, 0);
    assert_eq!(admin_balance, fees_from_swap + initial_admin_pool_tokens);
}

#[tokio::test]
pub async fn test_successful_withdraw_partial_balance() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_denominator: 100,
            host_fee_numerator: 1,
            trade_fee_denominator: 100,
            trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
        },
        InitialSupply {
            initial_supply_a: 100,
            initial_supply_b: 100,
        },
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let initial_admin_pool_tokens =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
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

    let fees_from_swap = token_operations::balance(&mut ctx, &pool.pool_token_fees_vault).await;
    let half_fees_from_swap = fees_from_swap / 2;
    client::withdraw_fees(&mut ctx, &pool, half_fees_from_swap)
        .await
        .unwrap();

    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.pool_token_fees_vault).await;
    let admin_balance =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;
    assert_eq!(fee_vault_balance, fees_from_swap - half_fees_from_swap);
    assert_eq!(
        admin_balance,
        half_fees_from_swap + initial_admin_pool_tokens
    );
}

#[tokio::test]
pub async fn test_withdraw_0_fails() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            host_fee_denominator: 100,
            host_fee_numerator: 1,
            trade_fee_denominator: 100,
            trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
        },
        InitialSupply {
            initial_supply_a: 100,
            initial_supply_b: 100,
        },
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
        Swap {
            amount_in: 50,
            minimum_amount_out: 47,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        client::withdraw_fees(&mut ctx, &pool, 0)
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
            host_fee_denominator: 100,
            host_fee_numerator: 1,
            trade_fee_denominator: 100,
            trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_withdraw_fee_denominator: 100,
            owner_withdraw_fee_numerator: 1,
        },
        InitialSupply {
            initial_supply_a: 100,
            initial_supply_b: 100,
        },
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    assert_eq!(
        client::withdraw_fees(&mut ctx, &pool, 10)
            .await
            .unwrap_err()
            .unwrap(),
        hyperplane_error!(SwapError::ZeroTradingTokens)
    );
}
