use anchor_lang::prelude::Pubkey;
use anchor_spl::token::{spl_token, spl_token::state::Mint};
use arrayref::array_ref;
use solana_program_test::BanksClientError;
use solana_sdk::system_instruction;
use solana_sdk::{
    program_error::ProgramError, program_pack::Pack, signer::Signer, transport::TransportError,
};

use super::{
    setup::{kp, KP},
    types::TestContext,
};
use crate::send_tx;

pub async fn create_token_account(
    ctx: &mut TestContext,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Pubkey, BanksClientError> {
    let account = kp();
    let rent = ctx.context.banks_client.get_rent().await.unwrap();

    send_tx!(
        ctx,
        [
            system_instruction::create_account(
                &ctx.context.payer.pubkey(),
                &account.pubkey(),
                rent.minimum_balance(spl_token::state::Account::LEN),
                spl_token::state::Account::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_account(
                &spl_token::id(),
                &account.pubkey(),
                mint,
                owner,
            )
            .unwrap()
        ],
        account.as_ref()
    )?;

    Ok(account.pubkey())
}

pub async fn create_mint(ctx: &mut TestContext, mint: &KP, decimals: u8) {
    send_tx!(
        ctx,
        [
            system_instruction::create_account(
                &ctx.context.payer.pubkey(),
                &mint.pubkey(),
                ctx.rent.minimum_balance(Mint::LEN),
                Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &mint.pubkey(),
                &ctx.context.payer.pubkey(),
                None,
                decimals,
            )
            .unwrap()
        ],
        mint.as_ref()
    )
    .unwrap();
}

pub async fn mint_to(
    ctx: &mut TestContext,
    mint: &Pubkey,
    mint_into_account: &Pubkey,
    amount: u64,
) -> Result<(), TransportError> {
    send_tx!(
        ctx,
        [spl_token::instruction::mint_to(
            &spl_token::id(),
            mint,
            mint_into_account,
            &ctx.context.payer.pubkey(),
            &[],
            amount,
        )
        .unwrap()],
    )?;

    Ok(())
}

fn check_data_len(data: &[u8], min_len: usize) -> Result<(), ProgramError> {
    if data.len() < min_len {
        Err(ProgramError::AccountDataTooSmall)
    } else {
        Ok(())
    }
}

fn get_token_balance(data: &[u8]) -> u64 {
    if let Err(_err) = check_data_len(data, spl_token::state::Account::get_packed_len()) {
        return 0;
    }
    let amount = array_ref![data, 64, 8];

    u64::from_le_bytes(*amount)
}

pub async fn balance(env: &mut TestContext, account: &Pubkey) -> u64 {
    let acc = env
        .context
        .banks_client
        .get_account(*account)
        .await
        .unwrap()
        .unwrap();

    get_token_balance(&acc.data)
}

pub async fn supply(env: &mut TestContext, mint: &Pubkey) -> u64 {
    let acc = env
        .context
        .banks_client
        .get_account(*mint)
        .await
        .unwrap()
        .unwrap();

    get_mint_supply(&acc.data)
}

fn get_mint_supply(data: &[u8]) -> u64 {
    if let Err(_err) = check_data_len(data, Mint::get_packed_len()) {
        return 0;
    }
    let supply = array_ref![data, 36, 8];

    u64::from_le_bytes(*supply)
}

pub async fn create_and_mint_to_token_account(
    ctx: &mut TestContext,
    owner: &Pubkey,
    mint: &Pubkey,
    amount: u64,
) -> Pubkey {
    let token_account = create_token_account(ctx, mint, owner).await.unwrap();
    mint_to(ctx, mint, &token_account, amount).await.unwrap();
    token_account
}
