#![allow(clippy::too_many_arguments)]

use solana_program_test::BanksClientError;
use solana_sdk::instruction::Instruction;
use solana_sdk::system_instruction;

use hyperplane::curve::calculator::TradeDirection;
use hyperplane::ix::Swap;
use hyperplane::state::SwapPool;
use hyperplane::{curve::fees::Fees, CurveUserParameters, InitialSupply};

use crate::send_tx;

use super::types::{PoolUserAccounts, SwapPoolAccounts, TestContext};

pub async fn initialize_pool(
    ctx: &mut TestContext,
    pool: &SwapPoolAccounts,
    fees: Fees,
    initial_supply: InitialSupply,
    curve_parameters: CurveUserParameters,
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
            instructions::initialize_pool(pool, fees, initial_supply, curve_parameters)
        ],
        pool.pool.as_ref(),
        pool.admin.admin.as_ref(),
        pool.admin.pool_token_ata.as_ref()
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

pub(crate) mod instructions {
    use solana_sdk::signer::Signer;

    use hyperplane::ix;

    use super::*;

    pub fn initialize_pool(
        pool: &SwapPoolAccounts,
        fees: Fees,
        initial_supply: InitialSupply,
        curve_parameters: CurveUserParameters,
    ) -> Instruction {
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
            fees,
            initial_supply,
            curve_parameters,
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
}
