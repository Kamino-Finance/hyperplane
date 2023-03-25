use crate::common::types::{SwapPoolAccounts, TestContext};
use crate::common::{client, setup};
use hyperplane::curve::fees::Fees;
use hyperplane::{CurveUserParameters, InitialSupply};
use solana_sdk::native_token::sol_to_lamports;

pub enum ProgramDependency {}

pub async fn new_pool(
    ctx: &mut TestContext,
    fees: Fees,
    initial_supply: InitialSupply,
    decimals: (u8, u8),
    params: CurveUserParameters,
) -> SwapPoolAccounts {
    let pool = setup::new_pool_accs(ctx, decimals, &initial_supply).await;

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
