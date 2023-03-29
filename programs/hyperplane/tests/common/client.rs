#![allow(clippy::too_many_arguments)]

use hyperplane::{
    curve::calculator::TradeDirection,
    ix::{DepositAllTokenTypes, Initialize, Swap, UpdatePoolConfig, WithdrawFees},
    state::SwapPool,
};
use solana_program_test::BanksClientError;
use solana_sdk::{instruction::Instruction, system_instruction};

use super::types::{PoolUserAccounts, SwapPoolAccounts, TestContext};
use crate::send_tx;

pub async fn initialize_pool(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    initialize: Initialize,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [
            system_instruction::create_account(
                &ctx.context.payer.pubkey(),
                &pool.pubkey(),
                ctx.rent.minimum_balance(SwapPool::LEN),
                SwapPool::LEN as u64,
                &hyperplane::id(),
            ),
            instructions::initialize_pool(pool, initialize)
        ],
        pool.pool.as_ref(),
        pool.admin.admin.as_ref(),
        pool.admin.pool_token_ata.as_ref()
    )
}

pub async fn deposit_all(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    user: &PoolUserAccounts,
    deposit_all: DepositAllTokenTypes,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::deposit_all(pool, user, deposit_all)],
        user.user.as_ref()
    )
}

pub async fn swap(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    user: &PoolUserAccounts,
    trade_direction: TradeDirection,
    swap: Swap,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::swap(pool, user, trade_direction, swap)],
        user.user.as_ref()
    )
}

pub async fn withdraw_fees(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    withdraw_fees: WithdrawFees,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::withdraw_fees(pool, withdraw_fees)],
        pool.admin.admin.as_ref()
    )
}

pub async fn update_pool_config(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    update_pool_config: UpdatePoolConfig,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::update_pool_config(pool, update_pool_config)],
        pool.admin.admin.as_ref()
    )
}

pub(crate) mod instructions {
    use hyperplane::{ix, ix::DepositAllTokenTypes};
    use solana_sdk::signer::Signer;

    use super::*;

    pub fn initialize_pool(pool: &SwapPoolAccounts, initialize: Initialize) -> Instruction {
        ix::initialize_pool(
            &hyperplane::id(),
            &pool.admin.pubkey(),
            &pool.pubkey(),
            &pool.curve,
            &pool.token_a_mint,
            &pool.token_b_mint,
            &pool.token_a_vault,
            &pool.token_b_vault,
            &pool.authority,
            &pool.pool_token_mint,
            &pool.pool_token_fees_vault,
            &pool.admin.token_a_ata,
            &pool.admin.token_b_ata,
            &pool.admin.pool_token_ata.pubkey(),
            &pool.pool_token_program,
            &pool.token_a_token_program,
            &pool.token_b_token_program,
            initialize,
        )
        .unwrap()
    }

    pub fn deposit_all(
        pool: &SwapPoolAccounts,
        user: &PoolUserAccounts,
        deposit_all: DepositAllTokenTypes,
    ) -> Instruction {
        ix::deposit_all_token_types(
            &hyperplane::id(),
            &pool.token_a_token_program,
            &pool.token_b_token_program,
            &pool.pool_token_program,
            &pool.pubkey(),
            &pool.authority,
            &user.pubkey(),
            &user.token_a_ata,
            &user.token_b_ata,
            &pool.token_a_vault,
            &pool.token_b_vault,
            &pool.pool_token_mint,
            &user.pool_token_ata,
            &pool.token_a_mint,
            &pool.token_b_mint,
            &pool.curve,
            deposit_all,
        )
        .unwrap()
    }

    pub fn swap(
        pool: &SwapPoolAccounts,
        user: &PoolUserAccounts,
        trade_direction: TradeDirection,
        swap: Swap,
    ) -> Instruction {
        let (
            (source_mint, source_token_program, pool_source_vault, user_source_ata),
            (
                destination_mint,
                destination_token_program,
                pool_destination_vault,
                user_destination_ata,
            ),
        ) = match trade_direction {
            TradeDirection::AtoB => (
                (
                    &pool.token_a_mint,
                    &pool.token_a_token_program,
                    &pool.token_a_vault,
                    &user.token_a_ata,
                ),
                (
                    &pool.token_b_mint,
                    &pool.token_b_token_program,
                    &pool.token_b_vault,
                    &user.token_b_ata,
                ),
            ),
            TradeDirection::BtoA => (
                (
                    &pool.token_b_mint,
                    &pool.token_b_token_program,
                    &pool.token_b_vault,
                    &user.token_b_ata,
                ),
                (
                    &pool.token_a_mint,
                    &pool.token_a_token_program,
                    &pool.token_a_vault,
                    &user.token_a_ata,
                ),
            ),
        };
        ix::swap(
            &hyperplane::id(),
            source_token_program,
            destination_token_program,
            &pool.pool_token_program,
            &pool.pubkey(),
            &pool.authority,
            &user.pubkey(),
            user_source_ata,
            pool_source_vault,
            pool_destination_vault,
            user_destination_ata,
            &pool.pool_token_mint,
            &pool.pool_token_fees_vault,
            source_mint,
            destination_mint,
            &pool.curve,
            None,
            swap,
        )
        .unwrap()
    }

    pub fn withdraw_fees(pool: &SwapPoolAccounts, withdraw_fees: WithdrawFees) -> Instruction {
        ix::withdraw_fees(
            &hyperplane::id(),
            &pool.admin.pubkey(),
            &pool.pubkey(),
            &pool.authority,
            &pool.pool_token_mint,
            &pool.pool_token_fees_vault,
            &pool.admin.pool_token_ata.pubkey(),
            &pool.pool_token_program,
            withdraw_fees,
        )
        .unwrap()
    }

    pub fn update_pool_config(
        pool: &SwapPoolAccounts,
        update_pool_config: UpdatePoolConfig,
    ) -> Instruction {
        ix::update_pool_config(
            &hyperplane::id(),
            &pool.admin.pubkey(),
            &pool.pubkey(),
            update_pool_config,
        )
        .unwrap()
    }
}
