mod common;

use common::{client, runner};
use hyperplane::ix::UpdatePoolConfig;
use hyperplane::{
    curve::{calculator::TradeDirection, fees::Fees},
    error::SwapError,
    ix::Swap,
    state::{SwapState, UpdatePoolConfigMode, UpdatePoolConfigValue},
    CurveUserParameters,
};
use solana_program_test::tokio::{self};

use crate::common::setup::default_supply;
use crate::common::{fixtures, setup, state, types::TradingTokenSpec};

#[tokio::test]
pub async fn test_swap_fails_with_withdrawal_only_mode() {
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
            UpdatePoolConfigMode::WithdrawalsOnlyMode,
            UpdatePoolConfigValue::Bool(true),
        ),
    )
    .await
    .unwrap();
    let pool_state = state::get_pool(&mut ctx, &pool).await;
    assert!(pool_state.withdrawals_only());

    let user = setup::new_pool_user(&mut ctx, &pool, (50, 0)).await;
    assert_eq!(
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
        .unwrap_err()
        .unwrap(),
        hyperplane_error!(SwapError::WithdrawalsOnlyMode)
    );

    // unset withdrawals_only mode
    client::update_pool_config(
        &mut ctx,
        &pool,
        UpdatePoolConfig::new(
            UpdatePoolConfigMode::WithdrawalsOnlyMode,
            UpdatePoolConfigValue::Bool(false),
        ),
    )
    .await
    .unwrap();
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
}
