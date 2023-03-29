use anchor_lang::prelude::Pubkey;
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
    cloned_account.set_data(account_to_clone.data);
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
    cloned_account.set_data(account_to_clone.data);
    test_context
        .context
        .set_account(new_address, &cloned_account);
}
