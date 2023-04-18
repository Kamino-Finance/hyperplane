mod common;

use anchor_lang::{prelude::ErrorCode, Id};
use anchor_spl::{token_2022::Token2022, token_interface::spl_token_2022::error::TokenError};
use common::{client, runner};
use hyperplane::{
    curve::fees::Fees, error::SwapError, ix::Withdraw, CurveUserParameters, InitialSupply,
};
use solana_program_test::tokio::{self};
use solana_sdk::signature::Signer;

use crate::common::{
    fixtures,
    fixtures::Sol,
    setup,
    setup::{kp, new_keypair},
    token_operations,
    token_operations::create_token_account,
    types::TradingTokenSpec,
    utils,
};

#[tokio::test]
pub async fn test_security_withdraw() {
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
    let lp = setup::new_lp_user(&mut ctx, &pool, (50, 50)).await;
    let lp_pool_tokens = token_operations::balance(&mut ctx, &lp.pool_token_ata).await;

    // wrong signer
    {
        let mut cloned_lp = lp.clone();
        cloned_lp.user = new_keypair(&mut ctx, Sol::one()).await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &pool,
                &cloned_lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
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
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
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
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::InvalidProgramAddress)
        );
    }

    // wrong token_a_mint
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_mint = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_mint, &cloned_pool.token_a_mint).await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintHasOne)
        );
    }

    // wrong token_b_mint
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_mint = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_b_mint, &cloned_pool.token_b_mint).await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintHasOne)
        );
    }

    // wrong token_a_vault
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_a_vault, &cloned_pool.token_a_vault).await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
        );
    }

    // wrong token_b_vault
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_vault = kp().pubkey();

        utils::clone_account(&mut ctx, &pool.token_b_vault, &cloned_pool.token_b_vault).await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectSwapAccount)
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
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectPoolMint)
        );
    }

    // wrong token_a_fees_vault
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
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectFeeAccount)
        );
    }

    // wrong token_b_fees_vault
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_fees_vault = kp().pubkey();

        utils::clone_account(
            &mut ctx,
            &pool.token_b_fees_vault,
            &cloned_pool.token_b_fees_vault,
        )
        .await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            hyperplane_error!(SwapError::IncorrectFeeAccount)
        );
    }

    // wrong token_a_user_ata authority
    {
        let mut cloned_lp = lp.clone();
        let wrong_authority = kp();

        cloned_lp.token_a_ata = create_token_account(
            &mut ctx,
            &pool.token_a_token_program,
            &pool.token_a_mint,
            &wrong_authority.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &pool,
                &cloned_lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenOwner)
        );
    }

    // wrong token_a_user_ata mint
    {
        let mut cloned_lp = lp.clone();
        let wrong_mint = kp();
        utils::clone_account(&mut ctx, &pool.token_a_mint, &wrong_mint.pubkey()).await;

        cloned_lp.token_a_ata = create_token_account(
            &mut ctx,
            &pool.token_a_token_program,
            &wrong_mint.pubkey(),
            &cloned_lp.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &pool,
                &cloned_lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong token_a_user_ata token_program
    {
        let mut cloned_lp = lp.clone();
        cloned_lp.token_a_ata = kp().pubkey();

        utils::clone_account_with_new_owner(
            &mut ctx,
            &lp.token_a_ata,
            &cloned_lp.token_a_ata,
            &Token2022::id(),
        )
        .await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &pool,
                &cloned_lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong token_b_user_ata authority
    {
        let mut cloned_lp = lp.clone();
        let wrong_authority = kp();
        cloned_lp.token_b_ata = create_token_account(
            &mut ctx,
            &pool.token_b_token_program,
            &pool.token_b_mint,
            &wrong_authority.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &pool,
                &cloned_lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenOwner)
        );
    }

    // wrong token_b_user_ata mint
    {
        let mut cloned_lp = lp.clone();
        let wrong_mint = kp();
        utils::clone_account(&mut ctx, &pool.token_b_mint, &wrong_mint.pubkey()).await;

        cloned_lp.token_b_ata = create_token_account(
            &mut ctx,
            &pool.token_b_token_program,
            &wrong_mint.pubkey(),
            &cloned_lp.pubkey(),
        )
        .await
        .unwrap();

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &pool,
                &cloned_lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenMint)
        );
    }

    // wrong token_b_user_ata token_program
    {
        let mut cloned_lp = lp.clone();
        cloned_lp.token_b_ata = kp().pubkey();

        utils::clone_account_with_new_owner(
            &mut ctx,
            &lp.token_b_ata,
            &cloned_lp.token_b_ata,
            &Token2022::id(),
        )
        .await;

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &pool,
                &cloned_lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong pool_token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.pool_token_program = Token2022::id();

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong token_a_token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_a_token_program = Token2022::id();

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }

    // wrong token_b_token_program
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.token_b_token_program = Token2022::id();

        assert_eq!(
            client::withdraw(
                &mut ctx,
                &cloned_pool,
                &lp,
                Withdraw::new(lp_pool_tokens, 1, 1)
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintTokenTokenProgram)
        );
    }
}
