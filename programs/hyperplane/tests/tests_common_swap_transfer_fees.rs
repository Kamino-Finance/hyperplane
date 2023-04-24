// useful for d.p. clarity in tests
#![allow(clippy::inconsistent_digit_grouping)]

mod common;

use common::{client, runner};
use hyperplane::{
    curve::{calculator::TradeDirection, fees::Fees},
    ix::Swap,
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};

use crate::common::{
    fixtures, setup, token_operations,
    types::{SwapPairSpec, TokenSpec},
};

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_transfer_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 0, // no owner trade fee
            owner_trade_fee_denominator: 0,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::default()),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 53);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 47);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_b_transfer_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 0, // no owner trade fee
            owner_trade_fee_denominator: 0,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::default(), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 53);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 149);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 47);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_b_transfer_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 0, // no owner trade fee
            owner_trade_fee_denominator: 0,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::default(), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 150);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 52);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 47);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_transfer_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 0, // no owner trade fee
            owner_trade_fee_denominator: 0,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::default()),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 52);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 150);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 47);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_and_b_transfer_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 0, // no owner trade fee
            owner_trade_fee_denominator: 0,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 53);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 46);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_and_b_transfer_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 0, // no owner trade fee
            owner_trade_fee_denominator: 0,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 53);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 149);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 46);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_and_b_transfer_fees_and_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 147);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 55);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 1);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 44);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_and_b_transfer_fees_and_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 55);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 147);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 1);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 44);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_transfer_fees_and_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), Default::default()),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 147);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 55);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 1);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 45);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_b_transfer_fees_and_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(Default::default(), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 55);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 147);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 1);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 45);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_b_transfer_fees_and_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(Default::default(), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 53);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 1);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 46);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_transfer_fees_and_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            trade_fee_numerator: 1,
            trade_fee_denominator: 100,
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), Default::default()),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 53);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 149);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 1);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 46);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_transfer_fees_and_owner_and_host_fees() {
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
            ..Default::default()
        },
        InitialSupply::new(1_000_000_000000, 1_000_000_000000),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), Default::default()),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50_000_000000, 0)).await;
    let host_fees = setup::new_pool_user(&mut ctx, &pool, (0, 0)).await;

    client::swap_with_host_fees(
        &mut ctx,
        &pool,
        &user,
        Some(&host_fees),
        TradeDirection::AtoB,
        Swap {
            amount_in: 50_000_000000,
            minimum_amount_out: 43_000_000000,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 1_049_450_500000);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 951_072_769042);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 494_505000);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    // host fees paid host fee account
    let host_token_a_fees_balance =
        token_operations::balance(&mut ctx, &host_fees.token_a_ata).await;
    assert_eq!(host_token_a_fees_balance, 4_995000);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 48_927_230958);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_b_transfer_fees_and_owner_and_host_fees() {
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
            ..Default::default()
        },
        InitialSupply::new(1_000_000_000000, 1_000_000_000000),
        SwapPairSpec::new(TokenSpec::default(), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50_000_000000)).await;
    let host_fees = setup::new_pool_user(&mut ctx, &pool, (0, 0)).await;

    client::swap_with_host_fees(
        &mut ctx,
        &pool,
        &user,
        Some(&host_fees),
        TradeDirection::BtoA,
        Swap {
            amount_in: 50_000_000000,
            minimum_amount_out: 43_000_000000,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 951_072_769042);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 1_049_450_500000);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 494_505000);

    // host fees paid host fee account
    let host_token_a_fees_balance =
        token_operations::balance(&mut ctx, &host_fees.token_b_ata).await;
    assert_eq!(host_token_a_fees_balance, 4_995000);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 48_927_230958);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_b_transfer_fees_and_owner_and_host_fees() {
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
            ..Default::default()
        },
        InitialSupply::new(1_000_000_000000, 1_000_000_000000),
        SwapPairSpec::new(Default::default(), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50_000_000000, 0)).await;
    let host_fees = setup::new_pool_user(&mut ctx, &pool, (0, 0)).await;

    client::swap_with_host_fees(
        &mut ctx,
        &pool,
        &user,
        Some(&host_fees),
        TradeDirection::AtoB,
        Swap {
            amount_in: 50_000_000000,
            minimum_amount_out: 43_000_000000,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 1_049_500_000000);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 951_023_816752);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 495_000000);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    // host fees paid host fee account
    let host_token_a_fees_balance =
        token_operations::balance(&mut ctx, &host_fees.token_a_ata).await;
    assert_eq!(host_token_a_fees_balance, 5_000000);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 48_927_207064);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_transfer_fees_and_owner_and_host_fees() {
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
            ..Default::default()
        },
        InitialSupply::new(1_000_000_000000, 1_000_000_000000),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::default()),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50_000_000000)).await;
    let host_fees = setup::new_pool_user(&mut ctx, &pool, (0, 0)).await;

    client::swap_with_host_fees(
        &mut ctx,
        &pool,
        &user,
        Some(&host_fees),
        TradeDirection::BtoA,
        Swap {
            amount_in: 50_000_000000,
            minimum_amount_out: 43_000_000000,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 951_023_816752);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 1_049_500_000000);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 495_000000);

    // host fees paid host fee account
    let host_token_a_fees_balance =
        token_operations::balance(&mut ctx, &host_fees.token_b_ata).await;
    assert_eq!(host_token_a_fees_balance, 5_000000);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 48_927_207064);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_and_b_transfer_fees_and_owner_and_host_fees() {
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
            ..Default::default()
        },
        InitialSupply::new(1_000_000_000000, 1_000_000_000000),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (50_000_000000, 0)).await;
    let host_fees = setup::new_pool_user(&mut ctx, &pool, (0, 0)).await;

    client::swap_with_host_fees(
        &mut ctx,
        &pool,
        &user,
        Some(&host_fees),
        TradeDirection::AtoB,
        Swap {
            amount_in: 50_000_000000,
            minimum_amount_out: 43_000_000000,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 1_049_450_500000);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 951_072_769042);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 494_505000);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    // host fees paid host fee account
    let host_token_a_fees_balance =
        token_operations::balance(&mut ctx, &host_fees.token_a_ata).await;
    assert_eq!(host_token_a_fees_balance, 4_995000);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 48_878_303727);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_and_b_transfer_fees_and_owner_and_host_fees() {
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
            ..Default::default()
        },
        InitialSupply::new(1_000_000_000000, 1_000_000_000000),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50_000_000000)).await;
    let host_fees = setup::new_pool_user(&mut ctx, &pool, (0, 0)).await;

    client::swap_with_host_fees(
        &mut ctx,
        &pool,
        &user,
        Some(&host_fees),
        TradeDirection::BtoA,
        Swap {
            amount_in: 50_000_000000,
            minimum_amount_out: 43_000_000000,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 951_072_769042);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 1_049_450_500000);

    // fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 494_505000);

    // host fees paid host fee account
    let host_token_a_fees_balance =
        token_operations::balance(&mut ctx, &host_fees.token_b_ata).await;
    assert_eq!(host_token_a_fees_balance, 4_995000);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 48_878_303727);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_transfer_fees_and_no_trade_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::default()),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 52);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 48);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_b_transfer_fees_and_no_trade_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::default(), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 52);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 149);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 48);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_b_transfer_fees_and_no_trade_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::default(), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 150);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 51);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 48);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_transfer_fees_and_no_trade_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::default()),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 51);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 150);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 48);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_and_b_transfer_fees_and_no_trade_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 52);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 47);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_and_b_transfer_fees_and_no_trade_fees() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 52);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 149);

    // 0% owner fees - nothing paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 47);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_transfer_fees_and_only_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::default()),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 147);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 54);

    // owner fee paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 1);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 46);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_b_transfer_fees_and_only_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::default(), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 54);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 147);

    // owner fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 1);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 46);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_b_transfer_fees_and_only_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::default(), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 52);

    // owner fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 1);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 47);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_transfer_fees_and_only_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::default()),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 52);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 149);

    // owner fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 1);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 47);
    assert_eq!(user_b_balance, 0);
}

#[tokio::test]
pub async fn test_swap_a_to_b_with_a_and_b_transfer_fees_and_only_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
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
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 147);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 54);

    // owner fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 1);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 0);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 45);
}

#[tokio::test]
pub async fn test_swap_b_to_a_with_a_and_b_transfer_fees_and_only_owner_fee() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees {
            owner_trade_fee_numerator: 1,
            owner_trade_fee_denominator: 100,
            ..Default::default()
        },
        InitialSupply::new(100, 100),
        SwapPairSpec::new(TokenSpec::transfer_fees(10), TokenSpec::transfer_fees(10)),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let user = setup::new_pool_user(&mut ctx, &pool, (0, 50)).await;

    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap {
            amount_in: 50,
            minimum_amount_out: 44,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 54);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 147);

    // owner fees paid into fee vault
    let token_a_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fees_vault_balance, 0);
    let token_b_fees_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fees_vault_balance, 1);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 45);
    assert_eq!(user_b_balance, 0);
}
