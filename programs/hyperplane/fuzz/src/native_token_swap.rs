//! Helpers for working with swaps in a fuzzing environment

use hyperplane::{
    curve::{base::SwapCurve, fees::Fees},
    instructions::model::CurveParameters,
    ix::{self, Deposit, Initialize, Swap, Withdraw, WithdrawFees},
    state::{Curve, SwapPool},
    utils::seeds,
    InitialSupply,
};
use solana_program::{
    bpf_loader, entrypoint::ProgramResult, program_pack::Pack, pubkey::Pubkey, rent::Rent,
    system_program, sysvar::Sysvar,
};
use solana_sdk::account::create_account_for_test;
use spl_token_2022::instruction::approve;

use crate::{
    native_account_data::NativeAccountData, native_processor::do_process_instruction, native_token,
    native_token::get_token_account_space,
};

pub struct NativeTokenSwap {
    pub admin: NativeAccountData,
    pub pool_authority_account: NativeAccountData,
    pub fees: Fees,
    pub swap_curve: SwapCurve,
    pub pool_account: NativeAccountData,
    pub swap_curve_account: NativeAccountData,
    pub pool_token_mint_account: NativeAccountData,
    pub token_a_fees_vault_account: NativeAccountData,
    pub token_b_fees_vault_account: NativeAccountData,
    pub admin_token_a_ata: NativeAccountData,
    pub admin_token_b_ata: NativeAccountData,
    pub admin_pool_token_ata: NativeAccountData,
    pub token_a_vault_account: NativeAccountData,
    pub token_a_mint_account: NativeAccountData,
    pub token_b_vault_account: NativeAccountData,
    pub token_b_mint_account: NativeAccountData,
    pub pool_token_program_account: NativeAccountData,
    pub token_a_program_account: NativeAccountData,
    pub token_b_program_account: NativeAccountData,
}

pub fn create_program_account(program_id: Pubkey) -> NativeAccountData {
    let mut account_data = NativeAccountData::new(0, bpf_loader::id());
    account_data.key = program_id;
    account_data.executable = true;
    account_data
}

pub fn create_sysvar_account<S: Sysvar>(sysvar: &S) -> NativeAccountData {
    let account = create_account_for_test(sysvar);

    NativeAccountData {
        key: S::id(),
        lamports: account.lamports,
        data: account.data,
        program_id: account.owner,
        is_signer: false,
        executable: account.executable,
    }
}

impl NativeTokenSwap {
    pub fn new(
        fees: Fees,
        curve_params: CurveParameters,
        token_a_amount: u64,
        token_b_amount: u64,
    ) -> Self {
        let mut admin_authority = NativeAccountData::new(0, system_program::id());
        admin_authority.is_signer = true;

        let (token_a_decimals, token_b_decimals) = match curve_params {
            CurveParameters::Stable {
                token_a_decimals,
                token_b_decimals,
                ..
            } => (token_a_decimals, token_b_decimals),
            _ => (6, 6),
        };

        let mut token_a_mint_account =
            native_token::create_mint(&admin_authority.key, token_a_decimals);
        let mut token_b_mint_account =
            native_token::create_mint(&admin_authority.key, token_b_decimals);

        let mut pool_account = NativeAccountData::new(SwapPool::LEN, hyperplane::id());
        let seeds::pda::InitPoolPdas {
            curve,
            authority,
            token_a_vault,
            token_b_vault,
            pool_token_mint,
            token_a_fees_vault,
            token_b_fees_vault,
        } = seeds::pda::init_pool_pdas(
            &pool_account.key,
            &token_a_mint_account.key,
            &token_b_mint_account.key,
        );

        let mut swap_curve_account =
            NativeAccountData::new_with_key(curve, Curve::LEN, hyperplane::id());
        let mut pool_authority_account = create_program_account(authority);
        let mut system_program_account = create_program_account(system_program::id());
        let mut rent = create_sysvar_account(&Rent::default());
        let mut pool_token_program_account = create_program_account(spl_token_2022::id());
        let mut token_b_program_account = create_program_account(spl_token::id());
        let mut token_a_program_account = create_program_account(spl_token::id());
        let mut pool_token_mint_account = NativeAccountData::new_with_key(
            pool_token_mint,
            spl_token_2022::state::Mint::LEN,
            spl_token_2022::id(),
        );

        let mut admin_authority_pool_token_ata =
            NativeAccountData::new(spl_token_2022::state::Account::LEN, spl_token_2022::id());

        let mut token_a_vault_account = NativeAccountData::new_with_key(
            token_a_vault,
            get_token_account_space(&token_a_program_account.key, &token_a_mint_account),
            token_a_program_account.key,
        );
        let mut token_b_vault_account = NativeAccountData::new_with_key(
            token_b_vault,
            get_token_account_space(&token_b_program_account.key, &token_b_mint_account),
            token_b_program_account.key,
        );
        let mut token_a_fees_vault_account = NativeAccountData::new_with_key(
            token_a_fees_vault,
            get_token_account_space(&token_a_program_account.key, &token_a_mint_account),
            token_a_program_account.key,
        );
        let mut token_b_fees_vault_account = NativeAccountData::new_with_key(
            token_b_fees_vault,
            get_token_account_space(&token_b_program_account.key, &token_b_mint_account),
            token_b_program_account.key,
        );
        let mut admin_authority_token_a_ata_account = native_token::create_token_account(
            &mut token_a_mint_account,
            &token_a_program_account.key,
            &admin_authority.key,
            token_a_amount,
        );
        let mut admin_authority_token_b_ata_account = native_token::create_token_account(
            &mut token_b_mint_account,
            &token_b_program_account.key,
            &admin_authority.key,
            token_b_amount,
        );

        let init_instruction = ix::initialize_pool(
            &hyperplane::id(),
            &admin_authority.key,
            &pool_account.key,
            &swap_curve_account.key,
            &token_a_mint_account.key,
            &token_b_mint_account.key,
            &token_a_vault_account.key,
            &token_b_vault_account.key,
            &pool_authority_account.key,
            &pool_token_mint_account.key,
            &token_a_fees_vault_account.key,
            &token_b_fees_vault_account.key,
            &admin_authority_token_a_ata_account.key,
            &admin_authority_token_b_ata_account.key,
            &admin_authority_pool_token_ata.key,
            &spl_token_2022::id(),
            &token_a_program_account.key,
            &token_b_program_account.key,
            Initialize {
                fees,
                curve_parameters: curve_params.clone().into(),
                initial_supply: InitialSupply::new(token_a_amount, token_b_amount),
            },
        )
        .unwrap();

        do_process_instruction(
            init_instruction,
            &[
                admin_authority.as_account_info(),
                pool_account.as_account_info(),
                swap_curve_account.as_account_info(),
                pool_authority_account.as_account_info(),
                token_a_mint_account.as_account_info(),
                token_b_mint_account.as_account_info(),
                token_a_vault_account.as_account_info(),
                token_b_vault_account.as_account_info(),
                pool_token_mint_account.as_account_info(),
                token_a_fees_vault_account.as_account_info(),
                token_b_fees_vault_account.as_account_info(),
                admin_authority_token_a_ata_account.as_account_info(),
                admin_authority_token_b_ata_account.as_account_info(),
                admin_authority_pool_token_ata.as_account_info(),
                system_program_account.as_account_info(),
                rent.as_account_info(),
                pool_token_program_account.as_account_info(),
                token_a_program_account.as_account_info(),
                token_b_program_account.as_account_info(),
            ],
        )
        .unwrap();

        Self {
            admin: admin_authority,
            pool_authority_account,
            fees,
            pool_account,
            swap_curve: SwapCurve::new_from_params(curve_params).unwrap(),
            swap_curve_account,
            pool_token_mint_account,
            token_a_fees_vault_account,
            token_b_fees_vault_account,
            admin_token_a_ata: admin_authority_token_a_ata_account,
            admin_token_b_ata: admin_authority_token_b_ata_account,
            admin_pool_token_ata: admin_authority_pool_token_ata,
            token_a_vault_account,
            token_a_mint_account,
            token_b_vault_account,
            token_b_mint_account,
            pool_token_program_account,
            token_a_program_account,
            token_b_program_account,
        }
    }

    pub fn create_pool_account(&mut self) -> NativeAccountData {
        native_token::create_token_account(
            &mut self.pool_token_mint_account,
            &self.pool_token_program_account.key,
            &self.admin.key,
            0,
        )
    }

    pub fn create_token_a_account(&mut self, amount: u64) -> NativeAccountData {
        native_token::create_token_account(
            &mut self.token_a_mint_account,
            &self.token_a_program_account.key,
            &self.admin.key,
            amount,
        )
    }

    pub fn create_token_b_account(&mut self, amount: u64) -> NativeAccountData {
        native_token::create_token_account(
            &mut self.token_b_mint_account,
            &self.token_b_program_account.key,
            &self.admin.key,
            amount,
        )
    }

    pub fn swap_a_to_b(
        &mut self,
        user_token_a_account: &mut NativeAccountData,
        user_token_b_account: &mut NativeAccountData,
        instruction: Swap,
    ) -> ProgramResult {
        let mut user_transfer_authority_account = NativeAccountData::new(0, system_program::id());
        user_transfer_authority_account.is_signer = true;
        do_process_instruction(
            approve(
                &self.token_a_program_account.key,
                &user_token_a_account.key,
                &user_transfer_authority_account.key,
                &self.admin.key,
                &[],
                instruction.amount_in,
            )
            .unwrap(),
            &[
                user_token_a_account.as_account_info(),
                user_transfer_authority_account.as_account_info(),
                self.admin.as_account_info(),
            ],
        )
        .unwrap();
        let swap_instruction = ix::swap(
            &hyperplane::id(),
            &user_transfer_authority_account.key,
            &self.pool_account.key,
            &self.swap_curve_account.key,
            &self.pool_authority_account.key,
            &self.token_a_mint_account.key,
            &self.token_b_mint_account.key,
            &self.token_a_vault_account.key,
            &self.token_b_vault_account.key,
            &self.token_a_fees_vault_account.key,
            &user_token_a_account.key,
            &user_token_b_account.key,
            Some(&self.admin_token_a_ata.key),
            &spl_token::id(),
            &spl_token::id(),
            instruction,
        )
        .unwrap();

        do_process_instruction(
            swap_instruction,
            &[
                self.admin.as_account_info(),
                self.pool_account.as_account_info(),
                self.swap_curve_account.as_account_info(),
                self.pool_authority_account.as_account_info(),
                self.token_a_mint_account.as_account_info(),
                self.token_b_mint_account.as_account_info(),
                self.token_a_vault_account.as_account_info(),
                self.token_b_vault_account.as_account_info(),
                self.token_a_fees_vault_account.as_account_info(),
                user_token_a_account.as_account_info(),
                user_token_b_account.as_account_info(),
                self.admin_token_a_ata.as_account_info(),
                self.token_a_program_account.as_account_info(),
                self.token_b_program_account.as_account_info(),
            ],
        )
    }

    pub fn swap_b_to_a(
        &mut self,
        user_token_b_account: &mut NativeAccountData,
        user_token_a_account: &mut NativeAccountData,
        instruction: Swap,
    ) -> ProgramResult {
        let mut user_transfer_authority_account = NativeAccountData::new(0, system_program::id());
        user_transfer_authority_account.is_signer = true;
        do_process_instruction(
            approve(
                &self.token_b_program_account.key,
                &user_token_b_account.key,
                &user_transfer_authority_account.key,
                &self.admin.key,
                &[],
                instruction.amount_in,
            )
            .unwrap(),
            &[
                user_token_b_account.as_account_info(),
                user_transfer_authority_account.as_account_info(),
                self.admin.as_account_info(),
            ],
        )
        .unwrap();

        let swap_instruction = ix::swap(
            &hyperplane::id(),
            &user_transfer_authority_account.key,
            &self.pool_account.key,
            &self.swap_curve_account.key,
            &self.pool_authority_account.key,
            &self.token_b_mint_account.key,
            &self.token_a_mint_account.key,
            &self.token_b_vault_account.key,
            &self.token_a_vault_account.key,
            &self.token_b_fees_vault_account.key,
            &user_token_b_account.key,
            &user_token_a_account.key,
            Some(&self.admin_token_b_ata.key),
            &spl_token::id(),
            &spl_token::id(),
            instruction,
        )
        .unwrap();

        do_process_instruction(
            swap_instruction,
            &[
                user_transfer_authority_account.as_account_info(),
                self.pool_account.as_account_info(),
                self.swap_curve_account.as_account_info(),
                self.pool_authority_account.as_account_info(),
                self.token_b_mint_account.as_account_info(),
                self.token_a_mint_account.as_account_info(),
                self.token_b_vault_account.as_account_info(),
                self.token_a_vault_account.as_account_info(),
                self.token_b_fees_vault_account.as_account_info(),
                user_token_b_account.as_account_info(),
                user_token_a_account.as_account_info(),
                self.admin_token_b_ata.as_account_info(),
                self.token_b_program_account.as_account_info(),
                self.token_a_program_account.as_account_info(),
            ],
        )
    }

    pub fn deposit(
        &mut self,
        user_token_a_account: &mut NativeAccountData,
        user_token_b_account: &mut NativeAccountData,
        user_pool_token_account: &mut NativeAccountData,
        mut instruction: Deposit,
    ) -> ProgramResult {
        let mut user_transfer_account = NativeAccountData::new(0, system_program::id());
        user_transfer_account.is_signer = true;
        do_process_instruction(
            approve(
                &self.token_a_program_account.key,
                &user_token_a_account.key,
                &user_transfer_account.key,
                &self.admin.key,
                &[],
                instruction.maximum_token_a_amount,
            )
            .unwrap(),
            &[
                user_token_a_account.as_account_info(),
                user_transfer_account.as_account_info(),
                self.admin.as_account_info(),
            ],
        )
        .unwrap();

        do_process_instruction(
            approve(
                &self.token_b_program_account.key,
                &user_token_b_account.key,
                &user_transfer_account.key,
                &self.admin.key,
                &[],
                instruction.maximum_token_b_amount,
            )
            .unwrap(),
            &[
                user_token_b_account.as_account_info(),
                user_transfer_account.as_account_info(),
                self.admin.as_account_info(),
            ],
        )
        .unwrap();

        // special logic: if we only deposit 1 pool token, we can't withdraw it
        // because we incur a withdrawal fee, so we hack it to not be 1
        if instruction.pool_token_amount == 1 {
            instruction.pool_token_amount = 2;
        }

        let deposit_instruction = ix::deposit(
            &hyperplane::id(),
            &user_transfer_account.key,
            &self.pool_account.key,
            &self.swap_curve_account.key,
            &self.pool_authority_account.key,
            &self.token_a_mint_account.key,
            &self.token_b_mint_account.key,
            &self.token_a_vault_account.key,
            &self.token_b_vault_account.key,
            &self.pool_token_mint_account.key,
            &user_token_a_account.key,
            &user_token_b_account.key,
            &user_pool_token_account.key,
            &self.pool_token_program_account.key,
            &spl_token::id(),
            &spl_token::id(),
            instruction,
        )
        .unwrap();

        do_process_instruction(
            deposit_instruction,
            &[
                user_transfer_account.as_account_info(),
                self.pool_account.as_account_info(),
                self.swap_curve_account.as_account_info(),
                self.pool_authority_account.as_account_info(),
                self.token_a_mint_account.as_account_info(),
                self.token_b_mint_account.as_account_info(),
                self.token_a_vault_account.as_account_info(),
                self.token_b_vault_account.as_account_info(),
                self.pool_token_mint_account.as_account_info(),
                user_token_a_account.as_account_info(),
                user_token_b_account.as_account_info(),
                user_pool_token_account.as_account_info(),
                self.pool_token_program_account.as_account_info(),
                self.token_a_program_account.as_account_info(),
                self.token_b_program_account.as_account_info(),
            ],
        )
    }

    pub fn withdraw(
        &mut self,
        user_pool_token_account: &mut NativeAccountData,
        user_token_a_account: &mut NativeAccountData,
        user_token_b_account: &mut NativeAccountData,
        mut instruction: Withdraw,
    ) -> ProgramResult {
        let mut user_transfer_account = NativeAccountData::new(0, system_program::id());
        user_transfer_account.is_signer = true;
        let pool_token_amount = native_token::get_token_balance(user_pool_token_account);
        // special logic to avoid withdrawing down to 1 pool token, which
        // eventually causes an error on withdrawing all
        if pool_token_amount.saturating_sub(instruction.pool_token_amount) == 1 {
            instruction.pool_token_amount = pool_token_amount;
        }
        do_process_instruction(
            approve(
                &self.pool_token_program_account.key,
                &user_pool_token_account.key,
                &user_transfer_account.key,
                &self.admin.key,
                &[],
                instruction.pool_token_amount,
            )
            .unwrap(),
            &[
                user_pool_token_account.as_account_info(),
                user_transfer_account.as_account_info(),
                self.admin.as_account_info(),
            ],
        )
        .unwrap();

        let withdraw_instruction = ix::withdraw(
            &hyperplane::id(),
            &user_transfer_account.key,
            &self.pool_account.key,
            &self.swap_curve_account.key,
            &self.pool_authority_account.key,
            &self.token_a_mint_account.key,
            &self.token_b_mint_account.key,
            &self.token_a_vault_account.key,
            &self.token_b_vault_account.key,
            &self.pool_token_mint_account.key,
            &self.token_a_fees_vault_account.key,
            &self.token_b_fees_vault_account.key,
            &user_token_a_account.key,
            &user_token_b_account.key,
            &user_pool_token_account.key,
            &self.pool_token_program_account.key,
            &spl_token::id(),
            &spl_token::id(),
            instruction,
        )
        .unwrap();

        do_process_instruction(
            withdraw_instruction,
            &[
                user_transfer_account.as_account_info(),
                self.pool_account.as_account_info(),
                self.swap_curve_account.as_account_info(),
                self.pool_authority_account.as_account_info(),
                self.token_a_mint_account.as_account_info(),
                self.token_b_mint_account.as_account_info(),
                self.token_a_vault_account.as_account_info(),
                self.token_b_vault_account.as_account_info(),
                self.pool_token_mint_account.as_account_info(),
                self.token_a_fees_vault_account.as_account_info(),
                self.token_b_fees_vault_account.as_account_info(),
                user_token_a_account.as_account_info(),
                user_token_b_account.as_account_info(),
                user_pool_token_account.as_account_info(),
                self.pool_token_program_account.as_account_info(),
                self.token_a_program_account.as_account_info(),
                self.token_b_program_account.as_account_info(),
            ],
        )
    }

    /// Burn all pool tokens from the given account
    pub fn withdraw_all(
        &mut self,
        pool_account: &mut NativeAccountData,
        token_a_account: &mut NativeAccountData,
        token_b_account: &mut NativeAccountData,
    ) -> ProgramResult {
        let pool_token_amount = native_token::get_token_balance(pool_account);
        if pool_token_amount > 0 {
            let instruction = Withdraw {
                pool_token_amount,
                minimum_token_a_amount: 0,
                minimum_token_b_amount: 0,
            };
            self.withdraw(pool_account, token_a_account, token_b_account, instruction)
        } else {
            Ok(())
        }
    }

    pub fn withdraw_a_fees(
        &mut self,
        admin_a_fees_ata: &mut NativeAccountData,
        instruction: WithdrawFees,
    ) -> ProgramResult {
        let withdraw_instruction = ix::withdraw_fees(
            &hyperplane::id(),
            &self.admin.key,
            &self.pool_account.key,
            &self.pool_authority_account.key,
            &self.token_a_mint_account.key,
            &self.token_a_fees_vault_account.key,
            &admin_a_fees_ata.key,
            &self.token_a_program_account.key,
            instruction,
        )
        .unwrap();

        do_process_instruction(
            withdraw_instruction,
            &[
                self.admin.as_account_info(),
                self.pool_account.as_account_info(),
                self.pool_authority_account.as_account_info(),
                self.token_a_mint_account.as_account_info(),
                self.token_a_fees_vault_account.as_account_info(),
                admin_a_fees_ata.as_account_info(),
                self.token_a_program_account.as_account_info(),
            ],
        )
    }

    pub fn withdraw_b_fees(
        &mut self,
        admin_b_fees_ata: &mut NativeAccountData,
        instruction: WithdrawFees,
    ) -> ProgramResult {
        let withdraw_instruction = ix::withdraw_fees(
            &hyperplane::id(),
            &self.admin.key,
            &self.pool_account.key,
            &self.pool_authority_account.key,
            &self.token_b_mint_account.key,
            &self.token_b_fees_vault_account.key,
            &admin_b_fees_ata.key,
            &self.token_b_program_account.key,
            instruction,
        )
        .unwrap();

        do_process_instruction(
            withdraw_instruction,
            &[
                self.admin.as_account_info(),
                self.pool_account.as_account_info(),
                self.pool_authority_account.as_account_info(),
                self.token_b_mint_account.as_account_info(),
                self.token_b_fees_vault_account.as_account_info(),
                admin_b_fees_ata.as_account_info(),
                self.token_b_program_account.as_account_info(),
            ],
        )
    }
}
