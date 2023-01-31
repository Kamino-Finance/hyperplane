use anchor_lang::prelude::borsh;
use anchor_lang::prelude::borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SwapEvent {
    DepositAllTokenTypes {
        token_a_amount: u64,
        token_b_amount: u64,
        pool_token_amount: u64,
    },
    DepositSingleTokenType {
        token_amount: u64,
        pool_token_amount: u64,
    },
    WithdrawAllTokenTypes {
        token_a_amount: u64,
        token_b_amount: u64,
        pool_token_amount: u64,
        fee: u64,
    },
    WithdrawSingleTokenType {
        token_amount: u64,
        pool_token_amount: u64,
        fee: u64,
    },
    Swap {
        token_in_amount: u64,
        token_out_amount: u64,
        fee: u64,
    },
}
