use std::sync::Arc;

use anchor_lang::Id;
use anchor_spl::token::Token;
use hyperplane::{ix::Deposit, utils::seeds, InitialSupply};
use solana_sdk::{signature::Keypair, signer::Signer, system_instruction};

use super::{fixtures::Sol, token_operations, types::TestContext};
use crate::{
    common::{
        client,
        types::{PoolAdminAccounts, PoolUserAccounts, SwapPairSpec, SwapPoolAccounts},
        utils::calculate_pool_tokens,
    },
    send_tx,
};

// ---------- KEYPAIR UTILS ----------

pub type KP = Arc<Keypair>;

pub fn kp() -> KP {
    Arc::new(Keypair::new())
}

pub async fn new_keypair(ctx: &mut TestContext, lamports: u64) -> Arc<Keypair> {
    let account = Keypair::new();
    send_tx!(
        ctx,
        [system_instruction::transfer(
            &ctx.context.payer.pubkey(),
            &account.pubkey(),
            lamports,
        )],
    )
    .unwrap();

    Arc::new(account)
}

// ---------- USER ABSTRACTIONS ----------

pub async fn new_pool_user(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    balances: (u64, u64),
) -> PoolUserAccounts {
    let user = new_keypair(ctx, Sol::one()).await;

    let token_a_ata = token_operations::create_token_account(
        ctx,
        &pool.token_a_token_program,
        &pool.token_a_mint,
        &user.pubkey(),
    )
    .await
    .unwrap();
    let token_b_ata = token_operations::create_token_account(
        ctx,
        &pool.token_b_token_program,
        &pool.token_b_mint,
        &user.pubkey(),
    )
    .await
    .unwrap();
    let pool_token_ata = token_operations::create_token_account(
        ctx,
        &pool.pool_token_program,
        &pool.pool_token_mint,
        &user.pubkey(),
    )
    .await
    .unwrap();

    if balances.0 > 0 {
        token_operations::mint_to(
            ctx,
            &pool.token_a_token_program,
            &pool.token_a_mint,
            &token_a_ata,
            balances.0,
        )
        .await
        .unwrap();
    }

    if balances.1 > 0 {
        token_operations::mint_to(
            ctx,
            &pool.token_b_token_program,
            &pool.token_b_mint,
            &token_b_ata,
            balances.1,
        )
        .await
        .unwrap();
    }

    PoolUserAccounts::new(user, token_a_ata, token_b_ata, pool_token_ata)
}

pub async fn new_lp_user(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    deposits: (u64, u64),
) -> PoolUserAccounts {
    let user = new_pool_user(ctx, pool, deposits).await;
    if deposits.0 > 0 || deposits.1 > 0 {
        let token_a_vault_balance = token_operations::balance(ctx, &pool.token_a_vault).await;
        let token_b_vault_balance = token_operations::balance(ctx, &pool.token_b_vault).await;
        let pool_token_supply = token_operations::supply(ctx, &pool.pool_token_mint).await;
        let (pool_tokens, a_deposit, b_deposit) = calculate_pool_tokens(
            deposits.0,
            deposits.1,
            token_a_vault_balance,
            token_b_vault_balance,
            pool_token_supply,
        );
        client::deposit(
            ctx,
            pool,
            &user,
            Deposit::new(pool_tokens, a_deposit, b_deposit),
        )
        .await
        .unwrap();
    }
    user
}

// ---------- PROGRAM STRUCTS UTILS ----------

pub async fn new_pool_accs(
    ctx: &mut TestContext,
    trading_tokens: SwapPairSpec,
    initial_supply: &InitialSupply,
) -> SwapPoolAccounts {
    let admin = new_keypair(ctx, Sol::from(100.0)).await;

    let token_a_mint = kp();
    let token_b_mint = kp();
    token_operations::create_mint(ctx, &token_a_mint, trading_tokens.a)
        .await
        .unwrap();
    token_operations::create_mint(ctx, &token_b_mint, trading_tokens.b)
        .await
        .unwrap();

    let pool = kp();

    let seeds::pda::InitPoolPdas {
        curve,
        authority,
        token_a_vault,
        token_b_vault,
        pool_token_mint,
        token_a_fees_vault,
        token_b_fees_vault,
    } = seeds::pda::init_pool_pdas(
        &pool.pubkey(),
        &token_a_mint.pubkey(),
        &token_b_mint.pubkey(),
    );

    let token_a_admin_ata = token_operations::create_and_mint_to_token_account(
        ctx,
        &trading_tokens.a.token_program,
        &admin.pubkey(),
        &token_a_mint.pubkey(),
        initial_supply.initial_supply_a,
    )
    .await;
    let token_b_admin_ata = token_operations::create_and_mint_to_token_account(
        ctx,
        &trading_tokens.b.token_program,
        &admin.pubkey(),
        &token_b_mint.pubkey(),
        initial_supply.initial_supply_b,
    )
    .await;
    let pool_token_admin_ata = kp();

    let admin = PoolAdminAccounts::new(
        admin.clone(),
        token_a_admin_ata,
        token_b_admin_ata,
        pool_token_admin_ata,
    );

    SwapPoolAccounts {
        admin,
        pool,
        curve,
        authority,
        token_a_mint: token_a_mint.pubkey(),
        token_b_mint: token_b_mint.pubkey(),
        pool_token_mint,
        token_a_vault,
        token_b_vault,
        token_a_fees_vault,
        token_b_fees_vault,
        pool_token_program: Token::id(),
        token_a_token_program: trading_tokens.a.token_program,
        token_b_token_program: trading_tokens.b.token_program,
    }
}

pub fn default_supply() -> InitialSupply {
    InitialSupply::new(1_000_000_000000, 1_000_000_000000)
}
