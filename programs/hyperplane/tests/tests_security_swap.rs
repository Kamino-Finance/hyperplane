use anchor_lang::{prelude::ErrorCode, Id};
use anchor_spl::{token_2022::Token2022, token_interface::spl_token_2022::error::TokenError};
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
    token_operations::create_token_account,
    types::TradingTokenSpec,
    utils,
};

mod common;

#[tokio::test]
pub async fn test_security_swap() {
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

    let user = setup::new_pool_user(&mut ctx, &pool, (51, 0)).await;
    let swap = Swap::new(50, 47);

    // wrong signer
    {
        let mut cloned_user = user.clone();
        cloned_user.user = new_keypair(&mut ctx, Sol::one()).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &pool,
                &cloned_user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            token_error!(TokenError::OwnerMismatch)
        );
    }

    // wrong swap_curve
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.curve = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.curve, &cloned_pool.curve).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
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
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::InvalidProgramAddress)
        );
    }

    // wrong source_mint a->b
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_mint = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_mint, &cloned_pool.token_a_mint).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong source_mint b->a
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_mint = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_b_mint, &cloned_pool.token_b_mint).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::BtoA,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong destination_mint a->b
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_mint = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_mint, &cloned_pool.token_a_mint).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong destination_mint b->a
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_mint = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_b_mint, &cloned_pool.token_b_mint).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::BtoA,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong destination_mint a->a
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_mint = pool.token_a_mint;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::RepeatedMint)
        );
    }

    // wrong destination_mint b->b
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_mint = pool.token_b_mint;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::BtoA,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::RepeatedMint)
        );
    }

    // wrong source_vault a->b
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_vault, &cloned_pool.token_a_vault).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
        );
    }

    // wrong source_vault b->a
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_b_vault, &cloned_pool.token_b_vault).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::BtoA,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
        );
    }

    // wrong destination_vault a->b
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_b_vault, &cloned_pool.token_b_vault).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
        );
    }

    // wrong destination_vault b->a
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_vault, &cloned_pool.token_a_vault).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::BtoA,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
        );
    }

    // wrong source_token_fees_vault a->b
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_vault, &cloned_pool.token_a_vault).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
        );
    }

    // wrong source_token_fees_vault b->a
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_b_vault, &cloned_pool.token_b_vault).await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::BtoA,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
        );
    }

    // wrong source_user_ata authority
    {
        let mut cloned_user = user.clone();
        let wrong_authority = kp();
        cloned_user.token_a_ata = create_token_account(
            &mut ctx,
            &pool.token_a_token_program,
            &pool.token_a_mint,
            &wrong_authority.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::swap(
                &mut ctx,
                &pool,
                &cloned_user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenOwner)
        );
    }

    // wrong source_user_ata mint
    {
        let wrong_mint = kp();
        utils::clone_account(&mut ctx, &pool.token_a_mint, &wrong_mint.pubkey()).await;

        let mut cloned_user = user.clone();
        cloned_user.token_a_ata = create_token_account(
            &mut ctx,
            &pool.token_a_token_program,
            &wrong_mint.pubkey(),
            &user.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::swap(
                &mut ctx,
                &pool,
                &cloned_user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong source_user_ata token_program
    {
        let mut cloned_user = user.clone();
        cloned_user.token_a_ata = kp().pubkey();

        utils::clone_account_with_new_owner(
            &mut ctx,
            &user.token_a_ata,
            &cloned_user.token_a_ata,
            &Token2022::id(),
        )
        .await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &pool,
                &cloned_user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong destination_user_ata authority
    {
        let mut cloned_user = user.clone();
        let wrong_authority = kp();
        cloned_user.token_b_ata = create_token_account(
            &mut ctx,
            &pool.token_b_token_program,
            &pool.token_b_mint,
            &wrong_authority.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::swap(
                &mut ctx,
                &pool,
                &cloned_user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenOwner)
        );
    }

    // wrong destination_user_ata mint
    {
        let wrong_mint = kp();
        utils::clone_account(&mut ctx, &pool.token_b_mint, &wrong_mint.pubkey()).await;

        let mut cloned_user = user.clone();
        cloned_user.token_b_ata = create_token_account(
            &mut ctx,
            &pool.token_b_token_program,
            &wrong_mint.pubkey(),
            &user.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::swap(
                &mut ctx,
                &pool,
                &cloned_user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong destination_user_ata token_program
    {
        let mut cloned_user = user.clone();
        cloned_user.token_b_ata = kp().pubkey();

        utils::clone_account_with_new_owner(
            &mut ctx,
            &user.token_b_ata,
            &cloned_user.token_b_ata,
            &Token2022::id(),
        )
        .await;

        assert_eq!(
            client::swap(
                &mut ctx,
                &pool,
                &cloned_user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong source_token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_token_program = Token2022::id();

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong destination_token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_token_program = Token2022::id();

        assert_eq!(
            client::swap(
                &mut ctx,
                &cloned_pool,
                &user,
                TradeDirection::AtoB,
                swap.clone()
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }
}
