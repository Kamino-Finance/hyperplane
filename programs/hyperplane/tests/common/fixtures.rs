use hyperplane::{curve::fees::Fees, CurveUserParameters, InitialSupply};
use solana_sdk::native_token::sol_to_lamports;

use crate::common::{
    client, setup,
    types::{SwapPoolAccounts, TestContext, TradingTokenSpec},
};

pub enum ProgramDependency {}

pub async fn new_pool(
    ctx: &mut TestContext,
    fees: Fees,
    initial_supply: InitialSupply,
    trading_tokens: TradingTokenSpec,
    params: CurveUserParameters,
) -> SwapPoolAccounts {
    let pool = setup::new_pool_accs(ctx, trading_tokens, &initial_supply).await;

    client::initialize_pool(ctx, &pool, fees, initial_supply, params)
        .await
        .unwrap();

    pool
}

pub struct Sol;
impl Sol {
    pub fn one() -> u64 {
        Self::from(1.0)
    }
    pub fn from(amt: f64) -> u64 {
        sol_to_lamports(amt)
    }
}
