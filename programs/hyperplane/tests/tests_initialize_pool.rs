mod common;

use common::{client, runner};
use hyperplane::{
    curve::{base::CurveType, calculator::INITIAL_SWAP_POOL_AMOUNT, fees::Fees},
    error::SwapError,
    utils::seeds,
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};
use solana_sdk::signer::Signer;

use crate::common::{fixtures, setup, state, token_operations, types::SwapPairSpec};

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
        InitialSupply::new(100, 100),
        SwapPairSpec::spl_tokens(6, 9),
        CurveUserParameters::ConstantProduct,
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
    assert_eq!(pool_state.curve_type, CurveType::ConstantProduct as u64);
    assert_eq!(pool_state.swap_curve, pool.curve);

    let _curve = state::get_constant_product_curve(&mut ctx, &pool).await;

    let vault_a_balance = token_operations::balance(&mut ctx, &pool.token_a_vault).await;
    assert_eq!(vault_a_balance, 100);
    let vault_b_balance = token_operations::balance(&mut ctx, &pool.token_b_vault).await;
    assert_eq!(vault_b_balance, 100);

    let admin_pool_token_balance =
        token_operations::balance(&mut ctx, &pool.admin.pool_token_ata.pubkey()).await;
    assert_eq!(admin_pool_token_balance, INITIAL_SWAP_POOL_AMOUNT as u64);
}

#[tokio::test]
pub async fn test_initialize_pool_with_same_token_a_and_b() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let initial_supply = InitialSupply::new(100, 100);
    let mut pool =
        setup::new_pool_accs(&mut ctx, SwapPairSpec::spl_tokens(9, 9), &initial_supply).await;

    pool.token_b_mint = pool.token_a_mint;
    let (token_b_vault, _token_b_vault_bump_seed) =
        seeds::pda::token_b_vault_pda(&pool.pubkey(), &pool.token_a_mint);
    pool.token_b_vault = token_b_vault;
    let (token_b_fees_vault, _token_b_fees_vault_bump_seed) =
        seeds::pda::token_b_fees_vault_pda(&pool.pubkey(), &pool.token_a_mint);
    pool.token_b_fees_vault = token_b_fees_vault;
    pool.admin.token_b_ata = pool.admin.token_a_ata;
    pool.token_b_token_program = pool.token_a_token_program;
    assert_eq!(
        client::initialize_pool(
            &mut ctx,
            &pool,
            hyperplane::ix::Initialize {
                fees: Fees::default(),
                initial_supply,
                curve_parameters: CurveUserParameters::Stable { amp: 100 },
            },
        )
        .await
        .unwrap_err()
        .unwrap(),
        hyperplane_error!(SwapError::RepeatedMint, 1)
    )
}

#[tokio::test]
pub async fn test_initialize_pool_with_same_token_b_and_a() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let initial_supply = InitialSupply::new(100, 100);
    let mut pool =
        setup::new_pool_accs(&mut ctx, SwapPairSpec::spl_tokens(9, 9), &initial_supply).await;

    pool.token_a_mint = pool.token_b_mint;
    let (token_a_vault, _token_a_vault_bump_seed) =
        seeds::pda::token_a_vault_pda(&pool.pubkey(), &pool.token_b_mint);
    pool.token_a_vault = token_a_vault;
    let (token_a_fees_vault, _token_a_fees_vault_bump_seed) =
        seeds::pda::token_a_fees_vault_pda(&pool.pubkey(), &pool.token_b_mint);
    pool.token_a_fees_vault = token_a_fees_vault;
    pool.admin.token_a_ata = pool.admin.token_b_ata;
    pool.token_a_token_program = pool.token_b_token_program;
    assert_eq!(
        client::initialize_pool(
            &mut ctx,
            &pool,
            hyperplane::ix::Initialize {
                fees: Fees::default(),
                initial_supply,
                curve_parameters: CurveUserParameters::Stable { amp: 100 },
            },
        )
        .await
        .unwrap_err()
        .unwrap(),
        hyperplane_error!(SwapError::RepeatedMint, 1)
    )
}
