use anchor_lang::{event, prelude::borsh, AnchorDeserialize, AnchorSerialize};

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepositAllTokenTypes {
    pub token_a_amount: u64,
    pub token_b_amount: u64,
    pub pool_token_amount: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepositSingleTokenType {
    pub token_amount: u64,
    pub pool_token_amount: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawAllTokenTypes {
    pub token_a_amount: u64,
    pub token_b_amount: u64,
    pub pool_token_amount: u64,
    pub fee: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawSingleTokenType {
    pub token_amount: u64,
    pub pool_token_amount: u64,
    pub fee: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Swap {
    pub token_in_amount: u64,
    pub token_out_amount: u64,
    pub fee: u64,
}

#[event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawFees {
    pub pool_token_amount: u64,
}
