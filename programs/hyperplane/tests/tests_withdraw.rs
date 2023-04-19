mod common;

use common::{client, runner};
use hyperplane::{
    curve::{calculator::INITIAL_SWAP_POOL_AMOUNT, fees::Fees},
    error::SwapError,
    ix::Withdraw,
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};
use solana_sdk::signer::Signer;

use crate::common::{fixtures, setup, token_operations, types::SwapPairSpec};

#[tokio::test]
pub async fn test_successful_withdraw_full_initial_balance_with_fees() {
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
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    client::withdraw(
        &mut ctx,
        &pool,
        &pool.admin.clone().into(),
        Withdraw::new(INITIAL_SWAP_POOL_AMOUNT as u64, 99, 99),
    )
    .await
    .unwrap();

    let pool_token_supply = token_operations::supply(&mut ctx, &pool.pool_token_mint).await;
    assert_eq!(pool_token_supply, 0);

    let admin_pool_token_balance =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;
    assert_eq!(admin_pool_token_balance, 0);
    let admin_token_a_balance = token_operations::balance(&mut ctx, &pool.admin.token_a_ata).await;
    assert_eq!(admin_token_a_balance, 99);
    let admin_token_b_balance = token_operations::balance(&mut ctx, &pool.admin.token_b_ata).await;
    assert_eq!(admin_token_b_balance, 99);

    let token_a_vault_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(token_a_vault_balance, 0);
    let token_b_vault_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(token_b_vault_balance, 0);

    let token_a_fee_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fee_vault_balance, 1);
    let token_b_fee_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fee_vault_balance, 1);
}

#[tokio::test]
pub async fn test_successful_withdraw_lp_user_full_balance_with_fees() {
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
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let lp = setup::new_lp_user(&mut ctx, &pool, (50, 50)).await;

    client::withdraw(
        &mut ctx,
        &pool,
        &lp,
        Withdraw::new(INITIAL_SWAP_POOL_AMOUNT as u64 / 2, 49, 49),
    )
    .await
    .unwrap();

    let pool_token_supply = token_operations::supply(&mut ctx, &pool.pool_token_mint).await;
    assert_eq!(pool_token_supply, INITIAL_SWAP_POOL_AMOUNT as u64);

    let lp_pool_token_balance = token_operations::balance(&mut ctx, &lp.pool_token_ata).await;
    assert_eq!(lp_pool_token_balance, 0);
    let lp_token_a_balance = token_operations::balance(&mut ctx, &lp.token_a_ata).await;
    assert_eq!(lp_token_a_balance, 49);
    let lp_token_b_balance = token_operations::balance(&mut ctx, &lp.token_b_ata).await;
    assert_eq!(lp_token_b_balance, 49);

    let token_a_vault_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(token_a_vault_balance, 100);
    let token_b_vault_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(token_b_vault_balance, 100);

    let token_a_fee_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fee_vault_balance, 1);
    let token_b_fee_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fee_vault_balance, 1);
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
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let lp = setup::new_lp_user(&mut ctx, &pool, (5_000_000, 5_000_000)).await;

    // fee = 0.01 * 2_500_000 = 25_000
    client::withdraw(
        &mut ctx,
        &pool,
        &lp,
        Withdraw::new(INITIAL_SWAP_POOL_AMOUNT as u64 / 4, 2_475_000, 2_475_000),
    )
    .await
    .unwrap();

    let pool_token_supply = token_operations::supply(&mut ctx, &pool.pool_token_mint).await;
    assert_eq!(
        pool_token_supply,
        ((INITIAL_SWAP_POOL_AMOUNT / 4) + INITIAL_SWAP_POOL_AMOUNT) as u64
    );

    let lp_pool_token_balance = token_operations::balance(&mut ctx, &lp.pool_token_ata).await;
    assert_eq!(lp_pool_token_balance, (INITIAL_SWAP_POOL_AMOUNT / 4) as u64);
    let lp_token_a_balance = token_operations::balance(&mut ctx, &lp.token_a_ata).await;
    assert_eq!(lp_token_a_balance, 2_475_000);
    let lp_token_b_balance = token_operations::balance(&mut ctx, &lp.token_b_ata).await;
    assert_eq!(lp_token_b_balance, 2_475_000);

    let token_a_vault_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(token_a_vault_balance, 12_500_000);
    let token_b_vault_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(token_b_vault_balance, 12_500_000);

    let token_a_fee_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_a_fees_vault).await;
    assert_eq!(token_a_fee_vault_balance, 25_000);
    let token_b_fee_vault_balance =
        token_operations::balance(&mut ctx, &pool.token_b_fees_vault).await;
    assert_eq!(token_b_fee_vault_balance, 25_000);
}

#[tokio::test]
pub async fn test_withdraw_more_than_owned_fails() {
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
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let lp = setup::new_lp_user(&mut ctx, &pool, (5_000_000, 5_000_000)).await;

    assert_eq!(
        client::withdraw(
            &mut ctx,
            &pool,
            &lp,
            Withdraw::new(INITIAL_SWAP_POOL_AMOUNT as u64, 2_475_000, 2_475_000)
        )
        .await
        .unwrap_err()
        .unwrap(),
        hyperplane_error!(SwapError::InsufficientPoolTokenFunds)
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
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;
    let lp = setup::new_lp_user(&mut ctx, &pool, (5_000_000, 5_000_000)).await;

    client::withdraw(
        &mut ctx,
        &pool,
        &lp,
        Withdraw::new(INITIAL_SWAP_POOL_AMOUNT as u64 / 4, 0, 0),
    )
    .await
    .unwrap();

    assert_eq!(
        client::withdraw(&mut ctx, &pool, &lp, Withdraw::new(0, 1, 1),)
            .await
            .unwrap_err()
            .unwrap(),
        hyperplane_error!(SwapError::ZeroTradingTokens)
    );
}
