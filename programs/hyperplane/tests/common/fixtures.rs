use hyperplane::{curve::fees::Fees, ix::Initialize, CurveUserParameters, InitialSupply};
use solana_sdk::native_token::sol_to_lamports;

use crate::common::{
    client, setup, token_operations,
    types::{SwapPairSpec, SwapPoolAccounts, TestContext},
};

pub enum ProgramDependency {}

pub async fn new_pool(
    ctx: &mut TestContext,
    fees: Fees,
    initial_supply: InitialSupply,
    trading_tokens: SwapPairSpec,
    curve_parameters: CurveUserParameters,
) -> SwapPoolAccounts {
    let initial_supply_a = token_operations::amount_with_transfer_fees(
        initial_supply.initial_supply_a,
        trading_tokens.a.transfer_fee_bps,
    );
    let initial_supply_b = token_operations::amount_with_transfer_fees(
        initial_supply.initial_supply_b,
        trading_tokens.b.transfer_fee_bps,
    );
    let initial_supply = InitialSupply::new(initial_supply_a, initial_supply_b);

    let pool = setup::new_pool_accs(ctx, trading_tokens, &initial_supply).await;

    client::initialize_pool(
        ctx,
        &pool,
        Initialize {
            fees,
            initial_supply,
            curve_parameters,
        },
    )
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
