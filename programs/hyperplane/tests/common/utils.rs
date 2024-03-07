use anchor_lang::prelude::Pubkey;
use hyperplane::curve::{calculator::RoundDirection, math::pool_tokens_to_trading_tokens};
use solana_sdk::account::AccountSharedData;

use crate::common::types::TestContext;

pub async fn clone_account(ctx: &mut TestContext, previous_address: &Pubkey, new_address: &Pubkey) {
    let account_to_clone = ctx
        .context
        .banks_client
        .get_account(*previous_address)
        .await
        .unwrap()
        .unwrap();
    let mut cloned_account = AccountSharedData::new(
        account_to_clone.lamports,
        account_to_clone.data.len(),
        &account_to_clone.owner,
    );
    cloned_account.set_data_from_slice(&account_to_clone.data);
    ctx.context.set_account(new_address, &cloned_account);
}

pub async fn clone_account_with_new_owner(
    test_context: &mut TestContext,
    previous_address: &Pubkey,
    new_address: &Pubkey,
    new_owner: &Pubkey,
) {
    let account_to_clone = test_context
        .context
        .banks_client
        .get_account(*previous_address)
        .await
        .unwrap()
        .unwrap();
    let mut cloned_account = AccountSharedData::new(
        account_to_clone.lamports,
        account_to_clone.data.len(),
        new_owner,
    );
    cloned_account.set_data_from_slice(&account_to_clone.data);
    test_context
        .context
        .set_account(new_address, &cloned_account);
}

pub fn calculate_pool_tokens(
    a_amount: u64,
    b_amount: u64,
    pool_token_a_amount: u64,
    pool_token_b_amount: u64,
    pool_token_supply: u64,
) -> (u64, u64, u64) {
    let a_share = a_amount as f64 / pool_token_a_amount as f64;
    let b_share = b_amount as f64 / pool_token_b_amount as f64;
    let min_share = a_share.min(b_share);

    let pool_tokens = min_share * pool_token_supply as f64;
    let pool_tokens = pool_tokens.trunc() as u64;

    let result = pool_tokens_to_trading_tokens(
        pool_tokens as u128,
        pool_token_supply as u128,
        pool_token_a_amount as u128,
        pool_token_b_amount as u128,
        RoundDirection::Floor,
    )
    .unwrap();
    (
        pool_tokens,
        result.token_a_amount as u64,
        result.token_b_amount as u64,
    )
}
