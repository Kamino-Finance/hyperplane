mod common;

use crate::common::{fixtures, setup, state, token_operations};
use common::{client, runner};
use hyperplane::curve::base::CurveType;
use hyperplane::curve::calculator::{TradeDirection, INITIAL_SWAP_POOL_AMOUNT};
use hyperplane::curve::fees::Fees;
use hyperplane::ix::Swap;
use hyperplane::utils::seeds;
use hyperplane::{CurveUserParameters, InitialSupply};
use solana_program_test::tokio::{self};
use solana_sdk::signer::Signer;

#[tokio::test]
pub async fn test_success_init_swap_pool() {
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
        InitialSupply {
            initial_supply_a: 100,
            initial_supply_b: 100,
        },
        (6, 9),
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
    assert_eq!(pool_state.pool_token_fees_vault, pool.pool_token_fees_vault);
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
pub async fn test_swap() {
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
        (6, 6),
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
    assert_eq!(vault_a_balance, 150);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 53);

    // supply increases due to fees payed in pool tokens
    let pool_token_supply = token_operations::supply(&mut ctx, &pool.pool_token_mint).await;
    assert_eq!(pool_token_supply, INITIAL_SWAP_POOL_AMOUNT as u64 + 4950495);
    let fee_vault_balance = token_operations::balance(&mut ctx, &pool.pool_token_fees_vault).await;
    // fees payed into fee vault
    assert_eq!(fee_vault_balance, 4950495);

    let user_a_balance = token_operations::balance(&mut ctx, &user.token_a_ata).await;
    let user_b_balance = token_operations::balance(&mut ctx, &user.token_b_ata).await;
    assert_eq!(user_a_balance, 0);
    assert_eq!(user_b_balance, 47);
}
