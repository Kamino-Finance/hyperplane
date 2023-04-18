mod common;

use common::{client, runner};
use hyperplane::{
    curve::{
        base::CurveType,
        calculator::{TradeDirection, INITIAL_SWAP_POOL_AMOUNT},
        fees::Fees,
    },
    ix::Swap,
    utils::seeds,
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};
use solana_sdk::signer::Signer;

use crate::common::{fixtures, setup, state, token_operations, types::TradingTokenSpec};

#[tokio::test]
pub async fn test_success_init_stable_swap_pool() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let fees = Fees {
        host_fee_denominator: 100,
        host_fee_numerator: 1,
        trade_fee_denominator: 100,
        trade_fee_numerator: 1,
        owner_trade_fee_denominator: 100,
        owner_trade_fee_numerator: 1,
        owner_withdraw_fee_denominator: 100,
        owner_withdraw_fee_numerator: 1,
    };
    let pool = fixtures::new_pool(
        &mut ctx,
        fees,
        InitialSupply::new(100, 100),
        TradingTokenSpec::new_spl_token(6, 9),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    let (pool_authority, pool_authority_bump_seed) = seeds::pda::pool_authority_pda(&pool.pubkey());

    let pool_state = state::get_pool(&mut ctx, &pool).await;
    assert_eq!(pool_state.admin, pool.admin.pubkey());
    assert_eq!(pool_state.pool_authority, pool.authority);
    assert_eq!(pool_state.pool_authority, pool_authority);
    assert_eq!(
        pool_state.pool_authority_bump_seed,
        pool_authority_bump_seed as u64
    );
    assert_eq!(pool_state.token_a_vault, pool.token_a_vault);
    assert_eq!(pool_state.token_b_vault, pool.token_b_vault);
    assert_eq!(pool_state.pool_token_mint, pool.pool_token_mint);
    assert_eq!(pool_state.token_a_mint, pool.token_a_mint);
    assert_eq!(pool_state.token_b_mint, pool.token_b_mint);
    assert_eq!(pool_state.token_a_fees_vault, pool.token_a_fees_vault);
    assert_eq!(pool_state.token_b_fees_vault, pool.token_b_fees_vault);
    assert_eq!(pool_state.fees, fees);
    assert_eq!(pool_state.curve_type, CurveType::Stable as u64);
    assert_eq!(pool_state.swap_curve, pool.curve);

    let curve = state::get_stable_curve(&mut ctx, &pool).await;
    assert_eq!(curve.amp, 100);
    assert_eq!(curve.token_a_factor, 1_000);
    assert_eq!(curve.token_b_factor, 1);

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 100);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 100);

    let admin_pool_token_balance =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;
    assert_eq!(admin_pool_token_balance, INITIAL_SWAP_POOL_AMOUNT as u64);
}

#[tokio::test]
pub async fn test_swap_a_to_b() {
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
        Swap {
            amount_in: 50,
            minimum_amount_out: 47,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 149);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 53);

    // fees payed into fee vault
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
pub async fn test_swap_b_to_a() {
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
        InitialSupply::new(100, 100),
        TradingTokenSpec::default(),
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
            minimum_amount_out: 47,
        },
    )
    .await
    .unwrap();

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 53);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 149);

    // fees payed into fee vault
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
