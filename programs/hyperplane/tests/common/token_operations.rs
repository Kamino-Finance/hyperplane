use anchor_lang::prelude::Pubkey;
use anchor_spl::token_2022::{
    spl_token_2022,
    spl_token_2022::{
        extension::{transfer_fee, transfer_fee::TransferFee, ExtensionType},
        pod::{PodU16, PodU64},
        state::{Account, Mint},
    },
};
use arrayref::array_ref;
use solana_program_test::BanksClientError;
use solana_sdk::{
    program_error::ProgramError, program_pack::Pack, signer::Signer, system_instruction,
    transport::TransportError,
};

use super::{
    setup::{kp, KP},
    types::TestContext,
};
use crate::{common::types::TokenSpec, send_tx};

pub async fn create_token_account(
    ctx: &mut TestContext,
    token_program: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Pubkey, BanksClientError> {
    let account = kp();
    create_token_account_kp(ctx, token_program, &account, mint, owner).await
}

pub async fn create_token_account_kp(
    ctx: &mut TestContext,
    token_program: &Pubkey,
    account: &KP,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Pubkey, BanksClientError> {
    let space = if token_program == &spl_token_2022::id() {
        ExtensionType::get_account_len::<Account>(&[ExtensionType::TransferFeeAmount])
    } else {
        Account::LEN
    };
    send_tx!(
        ctx,
        [
            system_instruction::create_account(
                &ctx.context.payer.pubkey(),
                &account.pubkey(),
                ctx.rent.minimum_balance(space),
                space as u64,
                token_program,
            ),
            spl_token_2022::instruction::initialize_account(
                token_program,
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

pub async fn create_mint(
    ctx: &mut TestContext,
    mint: &KP,
    TokenSpec {
        token_program,
        decimals,
        transfer_fee_bps,
    }: TokenSpec,
) -> Result<(), TransportError> {
    let is_transfer_fee = token_program == spl_token_2022::id() && transfer_fee_bps > 0;
    let space = if is_transfer_fee {
        ExtensionType::get_account_len::<Mint>(&[ExtensionType::TransferFeeConfig])
    } else if transfer_fee_bps > 0 {
        panic!(
            "Transfer fee not supported for token program (only token-2022): {}",
            token_program
        )
    } else {
        Mint::LEN
    };
    let mut ix = vec![system_instruction::create_account(
        &ctx.context.payer.pubkey(),
        &mint.pubkey(),
        ctx.rent.minimum_balance(space),
        space as u64,
        &token_program,
    )];

    if is_transfer_fee {
        ix.push(
            transfer_fee::instruction::initialize_transfer_fee_config(
                &token_program,
                &mint.pubkey(),
                None,
                None,
                transfer_fee_bps,
                u64::MAX,
            )
            .unwrap(),
        );
    }

    ix.push(
        spl_token_2022::instruction::initialize_mint(
            &token_program,
            &mint.pubkey(),
            &ctx.context.payer.pubkey(),
            None,
            decimals,
        )
        .unwrap(),
    );
    send_tx!(ctx, ix, mint.as_ref()).unwrap();
    Ok(())
}

pub async fn mint_to(
    ctx: &mut TestContext,
    token_program: &Pubkey,
    mint: &Pubkey,
    mint_into_account: &Pubkey,
    amount: u64,
) -> Result<(), TransportError> {
    send_tx!(
        ctx,
        [spl_token_2022::instruction::mint_to(
            token_program,
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
    if let Err(_err) = check_data_len(data, Account::get_packed_len()) {
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
    token_program: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
    amount: u64,
) -> Pubkey {
    let token_account = create_token_account(ctx, token_program, mint, owner)
        .await
        .unwrap();
    mint_to(ctx, token_program, mint, &token_account, amount)
        .await
        .unwrap();
    token_account
}

/// Returns the number of tokens needed to receive the desired amount after fees
pub fn amount_with_transfer_fees(desired_amount: u64, transfer_fee_bps: u16) -> u64 {
    if transfer_fee_bps > 0 {
        let fees = TransferFee {
            epoch: Default::default(),
            maximum_fee: PodU64::from(u64::MAX),
            transfer_fee_basis_points: PodU16::from(transfer_fee_bps),
        };
        let fee = fees.calculate_inverse_fee(desired_amount).unwrap();
        desired_amount + fee
    } else {
        desired_amount
    }
}
