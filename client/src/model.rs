use hyperplane::{curve::fees::Fees, CurveUserParameters, InitialSupply};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitializePoolConfig {
    pub token_a_mint: String,
    pub token_b_mint: String,
    pub curve: CurveUserParameters,
    pub fees: Fees,
    pub initial_supply: InitialSupply,
}
