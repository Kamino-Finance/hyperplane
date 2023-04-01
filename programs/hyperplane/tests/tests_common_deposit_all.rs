mod common;

use common::{client, runner};
use hyperplane::{
    curve::fees::Fees,
    error::SwapError,
    ix::{DepositAllTokenTypes, UpdatePoolConfig},
    state::{SwapState, UpdatePoolConfigMode, UpdatePoolConfigValue},
    CurveUserParameters,
};
use solana_program_test::tokio::{self};

use crate::common::{fixtures, setup, setup::default_supply, state, types::TradingTokenSpec};

#[tokio::test]
pub async fn test_deposit_all_fails_with_withdrawal_only_mode() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        default_supply(),
        TradingTokenSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    client::update_pool_config(
        &mut ctx,
        &pool,
        UpdatePoolConfig::new(
            UpdatePoolConfigMode::WithdrawalsOnly,
            UpdatePoolConfigValue::Bool(true),
        ),
    )
    .await
    .unwrap();
    let pool_state = state::get_pool(&mut ctx, &pool).await;
    assert!(pool_state.withdrawals_only());

    let user = setup::new_pool_user(&mut ctx, &pool, (1_000, 1_000)).await;
    assert_eq!(
        client::deposit_all(
            &mut ctx,
            &pool,
            &user,
            DepositAllTokenTypes {
                pool_token_amount: 1,
                maximum_token_a_amount: 1_000,
                maximum_token_b_amount: 1_000,
            },
        )
        .await
        .unwrap_err()
        .unwrap(),
        hyperplane_error!(SwapError::WithdrawalsOnlyMode)
    );

    // unset withdrawals_only mode
    client::update_pool_config(
        &mut ctx,
        &pool,
        UpdatePoolConfig::new(
            UpdatePoolConfigMode::WithdrawalsOnly,
            UpdatePoolConfigValue::Bool(false),
        ),
    )
    .await
    .unwrap();
    client::deposit_all(
        &mut ctx,
        &pool,
        &user,
        DepositAllTokenTypes {
            pool_token_amount: 1,
            maximum_token_a_amount: 1_000,
            maximum_token_b_amount: 1_000,
        },
    )
    .await
    .unwrap();
}
