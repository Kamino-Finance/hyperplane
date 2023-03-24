use anchor_lang::{prelude::*, solana_program::program_pack::Pack};
use anchor_spl::token_2022::{
    spl_token_2022,
    spl_token_2022::{
        extension::{
            transfer_fee::{instruction::initialize_transfer_fee_config, TransferFee},
            BaseStateWithExtensions, ExtensionType, StateWithExtensions,
        },
        instruction::{
            initialize_account, initialize_immutable_owner, initialize_mint,
            initialize_mint_close_authority, mint_to,
        },
        state::{Account, Mint},
    },
};
use solana_sdk::account::{create_account_for_test, Account as SolanaAccount};

use crate::instructions::test::runner::processor::do_process_instruction;

pub fn create_token_account(
    program_id: &Pubkey,
    mint_key: &Pubkey,
    mint_account: &mut SolanaAccount,
    mint_authority_key: &Pubkey,
    account_owner_key: &Pubkey,
    amount: u64,
) -> (Pubkey, SolanaAccount) {
    let account_key = Pubkey::new_unique();

    (
        account_key,
        create_token_account_with_address(
            &account_key,
            program_id,
            mint_key,
            mint_account,
            mint_authority_key,
            account_owner_key,
            amount,
        ),
    )
}

pub fn create_token_account_with_address(
    account_key: &Pubkey,
    program_id: &Pubkey,
    mint_key: &Pubkey,
    mint_account: &mut SolanaAccount,
    mint_authority_key: &Pubkey,
    account_owner_key: &Pubkey,
    amount: u64,
) -> SolanaAccount {
    let space = if *program_id == spl_token_2022::id() {
        ExtensionType::get_account_len::<Account>(&[
            ExtensionType::ImmutableOwner,
            ExtensionType::TransferFeeAmount,
        ])
    } else {
        Account::get_packed_len()
    };
    let minimum_balance = Rent::default().minimum_balance(space);
    let mut account_account = SolanaAccount::new(minimum_balance, space, program_id);
    let mut mint_authority_account = SolanaAccount::default();
    let mut rent_sysvar_account = create_account_for_test(&Rent::free());

    // no-ops in normal token, so we're good to run it either way
    do_process_instruction(
        initialize_immutable_owner(program_id, account_key).unwrap(),
        vec![&mut account_account],
    )
    .unwrap();

    do_process_instruction(
        initialize_account(program_id, account_key, mint_key, account_owner_key).unwrap(),
        vec![
            &mut account_account,
            mint_account,
            &mut mint_authority_account,
            &mut rent_sysvar_account,
        ],
    )
    .unwrap();

    if amount > 0 {
        do_process_instruction(
            mint_to(
                program_id,
                mint_key,
                account_key,
                mint_authority_key,
                &[],
                amount,
            )
            .unwrap(),
            vec![
                mint_account,
                &mut account_account,
                &mut mint_authority_account,
            ],
        )
        .unwrap();
    }

    account_account
}

pub fn create_mint(
    program_id: &Pubkey,
    authority_key: &Pubkey,
    freeze_authority: Option<&Pubkey>,
    close_authority: Option<&Pubkey>,
    fees: &TransferFee,
    decimals: u8,
) -> (Pubkey, SolanaAccount) {
    let mint_key = Pubkey::new_unique();

    (
        mint_key,
        create_mint_with_address(
            &mint_key,
            program_id,
            authority_key,
            freeze_authority,
            close_authority,
            decimals,
            fees,
        ),
    )
}

pub fn create_mint_with_address(
    mint_key: &Pubkey,
    program_id: &Pubkey,
    authority_key: &Pubkey,
    freeze_authority: Option<&Pubkey>,
    close_authority: Option<&Pubkey>,
    decimals: u8,
    fees: &TransferFee,
) -> SolanaAccount {
    let space = if *program_id == spl_token_2022::id() {
        if close_authority.is_some() {
            ExtensionType::get_account_len::<Mint>(&[
                ExtensionType::MintCloseAuthority,
                ExtensionType::TransferFeeConfig,
            ])
        } else {
            ExtensionType::get_account_len::<Mint>(&[ExtensionType::TransferFeeConfig])
        }
    } else {
        Mint::get_packed_len()
    };
    let minimum_balance = Rent::default().minimum_balance(space);
    let mut mint_account = SolanaAccount::new(minimum_balance, space, program_id);
    let mut rent_sysvar_account = create_account_for_test(&Rent::free());

    if *program_id == spl_token_2022::id() {
        if close_authority.is_some() {
            do_process_instruction(
                initialize_mint_close_authority(program_id, mint_key, close_authority).unwrap(),
                vec![&mut mint_account],
            )
            .unwrap();
        }
        do_process_instruction(
            initialize_transfer_fee_config(
                program_id,
                mint_key,
                freeze_authority,
                freeze_authority,
                fees.transfer_fee_basis_points.into(),
                fees.maximum_fee.into(),
            )
            .unwrap(),
            vec![&mut mint_account],
        )
        .unwrap();
    }
    do_process_instruction(
        initialize_mint(
            program_id,
            mint_key,
            authority_key,
            freeze_authority,
            decimals,
        )
        .unwrap(),
        vec![&mut mint_account, &mut rent_sysvar_account],
    )
    .unwrap();

    mint_account
}

pub fn get_token_account_space(token_program: &Pubkey, mint: &SolanaAccount) -> usize {
    if token_program == &spl_token_2022::id() {
        // calculate the space for the token account with required extensions
        let mint = StateWithExtensions::<Mint>::unpack(&mint.data).unwrap();
        let mint_extensions: Vec<ExtensionType> =
            BaseStateWithExtensions::get_extension_types(&mint).unwrap();

        let required_extensions =
            ExtensionType::get_required_init_account_extensions(&mint_extensions);

        ExtensionType::get_account_len::<Account>(&required_extensions)
    } else {
        anchor_spl::token::TokenAccount::LEN
    }
}
