mod common;

use anchor_lang::prelude::ErrorCode;
use common::{client, runner};
use hyperplane::{
    curve::fees::Fees,
    ix::UpdatePoolConfig,
    state::{UpdatePoolConfigMode, UpdatePoolConfigValue},
    CurveUserParameters,
};
use solana_program_test::tokio::{self};

use crate::common::{
    fixtures,
    fixtures::Sol,
    setup::{default_supply, new_keypair},
    types::SwapPairSpec,
};

#[tokio::test]
pub async fn test_security_update_swap_config() {
    let program = runner::program(&[]);
    let mut ctx = runner::start(program).await;

    let pool = fixtures::new_pool(
        &mut ctx,
        Fees::default(),
        default_supply(),
        SwapPairSpec::default(),
        CurveUserParameters::Stable { amp: 100 },
    )
    .await;

    // wrong admin
    {
        let mut cloned_pool = pool.clone();
        cloned_pool.admin.admin = new_keypair(&mut ctx, Sol::one()).await;

        assert_eq!(
            client::update_pool_config(
                &mut ctx,
                &cloned_pool,
                UpdatePoolConfig::new(
                    UpdatePoolConfigMode::WithdrawalsOnly,
                    UpdatePoolConfigValue::Bool(true),
                ),
            )
            .await
            .unwrap_err()
            .unwrap(),
            anchor_error!(ErrorCode::ConstraintHasOne)
        );
    }
}
