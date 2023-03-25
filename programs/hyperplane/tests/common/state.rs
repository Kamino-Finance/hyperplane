use anchor_lang::prelude::Pubkey;
use anchor_lang::{AccountDeserialize, Discriminator};
use hyperplane::state::{StableCurve, SwapPool};
use solana_sdk::account::Account;

use crate::common::types::{SwapPoolAccounts, TestContext, TestError};

pub async fn get_pool(ctx: &mut TestContext, pool: &SwapPoolAccounts) -> SwapPool {
    get::<SwapPool>(ctx, pool.pubkey()).await
}

pub async fn get_stable_curve(ctx: &mut TestContext, pool: &SwapPoolAccounts) -> StableCurve {
    get::<StableCurve>(ctx, pool.curve).await
}

pub async fn get<T: AccountDeserialize + Discriminator>(
    ctx: &mut TestContext,
    address: Pubkey,
) -> T {
    let acc = try_get::<T>(ctx, address).await;
    acc.unwrap()
}

pub async fn try_get<T: AccountDeserialize + Discriminator>(
    env: &mut TestContext,
    address: Pubkey,
) -> Result<T, TestError> {
    match env
        .context
        .banks_client
        .get_account(address)
        .await
        .map_err(|e| {
            println!("Error {:?}", e);
            TestError::UnknownError
        })? {
        Some(data) => deserialize::<T>(&data).map_err(|e| {
            println!("Error {:?}", e);
            TestError::CannotDeserialize
        }),
        None => Err(TestError::AccountNotFound),
    }
}

pub fn deserialize<T: AccountDeserialize + Discriminator>(
    account: &Account,
) -> Result<T, TestError> {
    let discriminator = &account.data[..8];
    if discriminator != T::discriminator() {
        return Err(TestError::BadDiscriminator);
    }

    let mut data: &[u8] = &account.data;
    let user: T = T::try_deserialize(&mut data).map_err(|_| TestError::CannotDeserialize)?;

    Ok(user)
}
