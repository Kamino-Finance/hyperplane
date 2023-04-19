mod common;

use anchor_lang::{prelude::ErrorCode, Id};
use anchor_spl::token_2022::Token2022;
use common::{client, runner};
use hyperplane::{
    curve::{calculator::TradeDirection, fees::Fees},
    error::SwapError,
    ix::{Swap, WithdrawFees},
    CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};
use solana_sdk::signature::Signer;

use crate::common::{
    fixtures,
    fixtures::Sol,
    setup,
    setup::{kp, new_keypair},
    token_operations::create_token_account,
    types::{AorB, SwapPairSpec},
    utils,
};

#[tokio::test]
pub async fn test_security_withdraw_fees() {
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
    client::swap(
        &mut ctx,
        &pool,
        &user,
        TradeDirection::BtoA,
        Swap::new(46, 1),
    )
    .await
    .unwrap();

    // wrong admin
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.admin.admin = new_keypair(&mut ctx, Sol::one()).await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
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
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
                .await
                .unwrap_err()
                .unwrap(),
            hyperplane_error!(SwapError::InvalidProgramAddress)
        );
    }

    // wrong fees_mint
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_mint = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_mint, &cloned_pool.token_a_mint).await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong fees_vault
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_fees_vault = kp().pubkey();

        utils::clone_account(
            &mut ctx,
            &pool.token_a_fees_vault,
            &cloned_pool.token_a_fees_vault,
        )
        .await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
                .await
                .unwrap_err()
                .unwrap(),
            hyperplane_error!(SwapError::IncorrectFeeAccount)
        );
    }

    // wrong admin_fees_ata authority
    {
        let mut cloned_pool = pool.clone();
        let wrong_authority = kp();

        cloned_pool.admin.token_a_ata = create_token_account(
            &mut ctx,
            &pool.token_a_token_program,
            &pool.token_a_mint,
            &wrong_authority.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenOwner)
        );
    }

    // wrong admin_fees_ata mint
    {
        let mut cloned_pool = pool.clone();
        let wrong_mint = kp();
        utils::clone_account(&mut ctx, &pool.token_a_mint, &wrong_mint.pubkey()).await;

        cloned_pool.admin.token_a_ata = create_token_account(
            &mut ctx,
            &pool.token_a_token_program,
            &wrong_mint.pubkey(),
            &pool.admin.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong admin_fees_ata token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.admin.token_a_ata = kp().pubkey();

        utils::clone_account_with_new_owner(
            &mut ctx,
            &pool.admin.token_a_ata,
            &cloned_pool.admin.token_a_ata,
            &Token2022::id(),
        )
        .await;

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong fees_token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_token_program = Token2022::id();

        assert_eq!(
            client::withdraw_fees(&mut ctx, &cloned_pool, AorB::A, WithdrawFees::new(10))
                .await
                .unwrap_err()
                .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }
}
