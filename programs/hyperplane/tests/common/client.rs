#![allow(clippy::too_many_arguments)]

use hyperplane::{
    curve::calculator::{AorB, TradeDirection},
    ix::{Deposit, Initialize, Swap, UpdatePoolConfig, Withdraw, WithdrawFees},
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

pub async fn deposit(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    user: &PoolUserAccounts,
    deposit: Deposit,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::deposit(pool, user, deposit)],
        user.user.as_ref()
    )
}

pub async fn swap_with_host_fees(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    user: &PoolUserAccounts,
    host_fees: Option<&PoolUserAccounts>,
    trade_direction: TradeDirection,
    swap: Swap,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::swap(
            pool,
            user,
            host_fees,
            trade_direction,
            swap
        )],
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
    swap_with_host_fees(ctx, pool, user, None, trade_direction, swap).await
}

pub async fn withdraw(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    user: &PoolUserAccounts,
    withdraw: Withdraw,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::withdraw(pool, user, withdraw)],
        user.user.as_ref()
    )
}

pub async fn withdraw_fees(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    a_or_b: AorB,
    withdraw_fees: WithdrawFees,
) -> Result<(), BanksClientError> {
    send_tx!(
        ctx,
        [instructions::withdraw_fees(pool, a_or_b, withdraw_fees)],
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
    use hyperplane::{ix, ix::Deposit};
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
            &pool.token_a_fees_vault,
            &pool.token_b_fees_vault,
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

    pub fn deposit(
        pool: &SwapPoolAccounts,
        user: &PoolUserAccounts,
        deposit: Deposit,
    ) -> Instruction {
        ix::deposit(
            &hyperplane::id(),
            &user.pubkey(),
            &pool.pubkey(),
            &pool.curve,
            &pool.authority,
            &pool.token_a_mint,
            &pool.token_b_mint,
            &pool.token_a_vault,
            &pool.token_b_vault,
            &pool.pool_token_mint,
            &user.token_a_ata,
            &user.token_b_ata,
            &user.pool_token_ata,
            &pool.pool_token_program,
            &pool.token_a_token_program,
            &pool.token_b_token_program,
            deposit,
        )
        .unwrap()
    }

    pub fn swap(
        pool: &SwapPoolAccounts,
        user: &PoolUserAccounts,
        host_fees: Option<&PoolUserAccounts>,
        trade_direction: TradeDirection,
        swap: Swap,
    ) -> Instruction {
        let (
            (
                source_mint,
                source_token_program,
                source_vault,
                source_fees_vault,
                user_source_ata,
                host_fees_source_ata,
            ),
            (destination_mint, destination_token_program, destination_vault, user_destination_ata),
        ) = match trade_direction {
            TradeDirection::AtoB => {
                let host_fees_source_ata = host_fees.map(|host_fees| &host_fees.token_a_ata);
                (
                    (
                        &pool.token_a_mint,
                        &pool.token_a_token_program,
                        &pool.token_a_vault,
                        &pool.token_a_fees_vault,
                        &user.token_a_ata,
                        host_fees_source_ata,
                    ),
                    (
                        &pool.token_b_mint,
                        &pool.token_b_token_program,
                        &pool.token_b_vault,
                        &user.token_b_ata,
                    ),
                )
            }
            TradeDirection::BtoA => {
                let host_fees_source_ata = host_fees.map(|host_fees| &host_fees.token_b_ata);
                (
                    (
                        &pool.token_b_mint,
                        &pool.token_b_token_program,
                        &pool.token_b_vault,
                        &pool.token_b_fees_vault,
                        &user.token_b_ata,
                        host_fees_source_ata,
                    ),
                    (
                        &pool.token_a_mint,
                        &pool.token_a_token_program,
                        &pool.token_a_vault,
                        &user.token_a_ata,
                    ),
                )
            }
        };
        ix::swap(
            &hyperplane::id(),
            &user.pubkey(),
            &pool.pubkey(),
            &pool.curve,
            &pool.authority,
            source_mint,
            destination_mint,
            source_vault,
            destination_vault,
            source_fees_vault,
            user_source_ata,
            user_destination_ata,
            host_fees_source_ata,
            source_token_program,
            destination_token_program,
            swap,
        )
        .unwrap()
    }

    pub fn withdraw(
        pool: &SwapPoolAccounts,
        user: &PoolUserAccounts,
        withdraw: Withdraw,
    ) -> Instruction {
        ix::withdraw(
            &hyperplane::id(),
            &user.pubkey(),
            &pool.pubkey(),
            &pool.curve,
            &pool.authority,
            &pool.token_a_mint,
            &pool.token_b_mint,
            &pool.token_a_vault,
            &pool.token_b_vault,
            &pool.pool_token_mint,
            &pool.token_a_fees_vault,
            &pool.token_b_fees_vault,
            &user.token_a_ata,
            &user.token_b_ata,
            &user.pool_token_ata,
            &pool.pool_token_program,
            &pool.token_a_token_program,
            &pool.token_b_token_program,
            withdraw,
        )
        .unwrap()
    }

    pub fn withdraw_fees(
        pool: &SwapPoolAccounts,
        a_or_b: AorB,
        withdraw_fees: WithdrawFees,
    ) -> Instruction {
        let (fees_mint, fees_vault, admin_fees_ata, fees_token_program) = match a_or_b {
            AorB::A => (
                &pool.token_a_mint,
                &pool.token_a_fees_vault,
                &pool.admin.token_a_ata,
                &pool.token_a_token_program,
            ),
            AorB::B => (
                &pool.token_b_mint,
                &pool.token_b_fees_vault,
                &pool.admin.token_b_ata,
                &pool.token_b_token_program,
            ),
        };

        ix::withdraw_fees(
            &hyperplane::id(),
            &pool.admin.pubkey(),
            &pool.pubkey(),
            &pool.authority,
            fees_mint,
            fees_vault,
            admin_fees_ata,
            fees_token_program,
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
