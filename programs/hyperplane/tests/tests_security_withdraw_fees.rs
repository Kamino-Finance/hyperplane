mod common;

use anchor_lang::{prelude::ErrorCode, Id};
use anchor_spl::token_2022::Token2022;
use common::{client, runner};
use hyperplane::{
    curve::{calculator::TradeDirection, fees::Fees},
    error::SwapError,
    ix::Swap,
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};
use solana_sdk::signature::Signer;

use crate::common::{
    fixtures,
    fixtures::Sol,
    setup,
    setup::{kp, new_keypair},
    token_operations::create_token_account_kp,
    types::TradingTokenSpec,
    utils,
};

#[tokio::test]
pub async fn test_security_withdraw_fees() {
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

    // wrong admin
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.admin.admin = new_keypair(&mut ctx, Sol::one()).await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintHasOne)
        );
    }

    // wrong pool_authority
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.authority = kp().pubkey();

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            hyperplane_error!(SwapError::InvalidProgramAddress)
        );
    }

    // wrong pool_token_mint
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.pool_token_mint = kp().pubkey();

        utils::clone_account(
            &mut ctx,
            &pool.pool_token_mint,
            &cloned_pool.pool_token_mint,
        )
        .await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            hyperplane_error!(SwapError::IncorrectPoolMint)
        );
    }

    // wrong pool_token_fees_vault
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.pool_token_fees_vault = kp().pubkey();

        utils::clone_account(
            &mut ctx,
            &pool.pool_token_fees_vault,
            &cloned_pool.pool_token_fees_vault,
        )
        .await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            hyperplane_error!(SwapError::IncorrectFeeAccount)
        );
    }

    // wrong pool_token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.pool_token_program = Token2022::id();

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong admin_pool_token_ata authority
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.admin.pool_token_ata = kp();

        let wrong_authority = kp();
        create_token_account_kp(
            &mut ctx,
            &pool.pool_token_program,
            &cloned_pool.admin.pool_token_ata,
            &pool.pool_token_mint,
            &wrong_authority.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenOwner)
        );
    }

    // wrong admin_pool_token_ata mint
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.admin.pool_token_ata = kp();

        let wrong_mint = kp();
        utils::clone_account(&mut ctx, &pool.pool_token_mint, &wrong_mint.pubkey()).await;

        create_token_account_kp(
            &mut ctx,
            &pool.pool_token_program,
            &cloned_pool.admin.pool_token_ata,
            &wrong_mint.pubkey(),
            &pool.admin.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong admin_pool_token_ata token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.admin.pool_token_ata = kp();

        utils::clone_account_with_new_owner(
            &mut ctx,
            &pool.admin.pool_token_ata.pubkey(),
            &cloned_pool.admin.pool_token_ata.pubkey(),
            &Token2022::id(),
        )
        .await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, 10)
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }
}
