use crate::common::runner::warp_two_slots;
use crate::common::types::{PoolAdminAccounts, PoolUserAccounts, SwapPoolAccounts};
use anchor_lang::Id;
use anchor_spl::token::Token;
use hyperplane::utils::seeds;
use hyperplane::InitialSupply;
use solana_sdk::{
    commitment_config::CommitmentLevel, signature::Keypair, signer::Signer, system_instruction,
    system_program, transaction::Transaction,
};
use std::sync::Arc;

use super::{fixtures::Sol, token_operations, types::TestContext};

// ---------- KEYPAIR UTILS ----------

pub type KP = Arc<Keypair>;
pub fn kp() -> KP {
    Arc::new(Keypair::new())
}

pub async fn new_keypair(ctx: &mut TestContext, min_lamports: u64) -> Arc<Keypair> {
    let account = Keypair::new();
    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::create_account(
            &ctx.context.payer.pubkey(),
            &account.pubkey(),
            min_lamports,
            0,
            &system_program::id(),
        )],
        Some(&ctx.context.payer.pubkey()),
        &[&ctx.context.payer, &account],
        ctx.context
            .banks_client
            .get_latest_blockhash()
            .await
            .unwrap(),
    );

    ctx.context
        .banks_client
        .process_transaction_with_commitment(transaction, CommitmentLevel::Processed)
        .await
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

    let token_a_ata =
        token_operations::create_token_account(ctx, &pool.token_a_mint, &user.pubkey())
            .await
            .unwrap();
    let token_b_ata =
        token_operations::create_token_account(ctx, &pool.token_b_mint, &user.pubkey())
            .await
            .unwrap();
    let pool_token_ata =
        token_operations::create_token_account(ctx, &pool.pool_token_mint, &user.pubkey())
            .await
            .unwrap();

    if balances.0 > 0 {
        token_operations::mint_to(ctx, &pool.token_a_mint, &token_a_ata, balances.0)
            .await
            .unwrap();
    }

    if balances.1 > 0 {
        token_operations::mint_to(ctx, &pool.token_b_mint, &token_b_ata, balances.1)
            .await
            .unwrap();
    }

    PoolUserAccounts::new(user, token_a_ata, token_b_ata, pool_token_ata)
}

// ---------- PROGRAM STRUCTS UTILS ----------

pub async fn new_pool_accs(
    ctx: &mut TestContext,
    decimals: (u8, u8),
    initial_supply: &InitialSupply,
) -> SwapPoolAccounts {
    let admin = new_keypair(ctx, Sol::from(100.0)).await;

    let token_a_mint = kp();
    let token_b_mint = kp();
    token_operations::create_mint(ctx, &token_a_mint, decimals.0).await;
    token_operations::create_mint(ctx, &token_b_mint, decimals.1).await;

    let pool = kp();

    let seeds::pda::InitPoolPdas {
        curve,
        authority,
        token_a_vault,
        token_b_vault,
        pool_token_mint,
        pool_token_fees_vault,
    } = seeds::pda::init_pool_pdas(
        &pool.pubkey(),
        &token_a_mint.pubkey(),
        &token_b_mint.pubkey(),
    );

    let token_a_admin_ata = token_operations::create_and_mint_to_token_account(
        ctx,
        &admin.pubkey(),
        &token_a_mint.pubkey(),
        initial_supply.initial_supply_a,
    )
    .await;
    let token_b_admin_ata = token_operations::create_and_mint_to_token_account(
        ctx,
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

    // prevent too many txs for a block
    warp_two_slots(ctx).await;

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
        pool_token_fees_vault,
        pool_token_program: Token::id(),
        token_a_token_program: Token::id(),
        token_b_token_program: Token::id(),
    }
}
