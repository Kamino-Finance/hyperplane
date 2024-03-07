use anchor_lang::solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, instruction::Instruction,
    program_error::ProgramError, program_pack::Pack, pubkey::Pubkey, rent::Rent,
};
use anchor_spl::{
    token::spl_token,
    token_2022::{
        spl_token_2022,
        spl_token_2022::{extension::transfer_fee::TransferFee, instruction::approve},
    },
};
use solana_sdk::account::{create_account_for_test, Account as SolanaAccount, WritableAccount};

use crate::{
    constraints::{SwapConstraints, SWAP_CONSTRAINTS},
    curve::{base::SwapCurve, fees::Fees},
    instructions::{
        model::CurveParameters,
        test::runner::{syscall_stubs::test_syscall_stubs, token},
    },
    ix,
    ix::Initialize,
    state::SwapPool,
    utils::seeds,
    InitialSupply,
};

// todo - xfer fees
#[derive(Default)]
pub struct SwapTransferFees {
    pub _pool_token: TransferFee,
    pub token_a: TransferFee,
    pub token_b: TransferFee,
}

pub struct SwapAccountInfo {
    pub admin_authority: Pubkey,
    pub pool_authority_bump_seed: u8,
    pub pool_authority: Pubkey,
    pub fees: Fees,
    pub initial_supply: InitialSupply,
    pub transfer_fees: SwapTransferFees,
    pub pool: Pubkey,
    pub pool_account: SolanaAccount,
    pub swap_curve_key: Pubkey,
    pub swap_curve_account: SolanaAccount,
    pub swap_curve: SwapCurve,
    pub curve_params: CurveParameters,
    pub pool_token_mint_key: Pubkey,
    pub pool_token_mint_account: SolanaAccount,
    pub token_a_fees_vault_key: Pubkey,
    pub token_a_fees_vault_account: SolanaAccount,
    pub token_b_fees_vault_key: Pubkey,
    pub token_b_fees_vault_account: SolanaAccount,
    pub admin_authority_token_a_ata_key: Pubkey,
    pub admin_authority_token_a_ata_account: SolanaAccount,
    pub admin_authority_token_b_ata_key: Pubkey,
    pub admin_authority_token_b_ata_account: SolanaAccount,
    pub admin_authority_pool_token_ata_key: Pubkey,
    pub admin_authority_pool_token_ata_account: SolanaAccount,
    pub token_a_vault_key: Pubkey,
    pub token_a_vault_account: SolanaAccount,
    pub token_a_mint_key: Pubkey,
    pub token_a_mint_account: SolanaAccount,
    pub token_b_vault_key: Pubkey,
    pub token_b_vault_account: SolanaAccount,
    pub token_b_mint_key: Pubkey,
    pub token_b_mint_account: SolanaAccount,
    pub pool_token_program_id: Pubkey,
    pub token_a_program_id: Pubkey,
    pub token_b_program_id: Pubkey,
}

impl SwapAccountInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        admin_authority: &Pubkey,
        fees: Fees,
        transfer_fees: SwapTransferFees,
        curve_params: CurveParameters,
        initial_supply: InitialSupply,
        pool_token_program_id: &Pubkey,
        token_a_program_id: &Pubkey,
        token_b_program_id: &Pubkey,
    ) -> Self {
        let InitialSupply {
            initial_supply_a,
            initial_supply_b,
        } = initial_supply;
        let pool = Pubkey::new_unique();
        let pool_account = SolanaAccount::new(u32::MAX as u64, SwapPool::LEN, &crate::id());
        let (swap_curve_key, _swap_curve_bump_seed) =
            Pubkey::find_program_address(&[seeds::SWAP_CURVE, pool.as_ref()], &crate::id());
        let swap_curve_account =
            SolanaAccount::new(u32::MAX as u64, crate::state::Curve::LEN, &crate::id());
        let (pool_authority, pool_authority_bump_seed) = seeds::pda::pool_authority_pda(&pool);

        let (pool_token_mint_key, _pool_token_mint_bump_seed) =
            Pubkey::find_program_address(&[seeds::POOL_TOKEN_MINT, pool.as_ref()], &crate::id());

        let pool_token_mint_account = SolanaAccount::new(
            u32::MAX as u64,
            spl_token_2022::state::Mint::LEN,
            pool_token_program_id, // this should be system but we no-op the system program calls
        );

        let admin_authority_pool_token_ata_key = Pubkey::new_unique();
        let admin_authority_pool_token_ata_account = SolanaAccount::new(
            u32::MAX as u64,
            spl_token_2022::state::Account::LEN,
            pool_token_program_id, // this should be system but we no-op the system program calls
        );

        let (token_a_decimals, token_b_decimals) = match curve_params {
            CurveParameters::Stable {
                token_a_decimals,
                token_b_decimals,
                ..
            } => (token_a_decimals, token_b_decimals),
            _ => (6, 6),
        };
        let (token_a_mint_key, mut token_a_mint_account) = token::create_mint(
            token_a_program_id,
            admin_authority,
            None,
            None,
            &transfer_fees.token_a,
            token_a_decimals,
        );
        let (token_a_vault_key, _token_a_vault_bump_seed) =
            seeds::pda::token_a_vault_pda(&pool, &token_a_mint_key);
        let token_a_vault_account = SolanaAccount::new(
            u32::MAX as u64,
            token::get_token_account_space(token_a_program_id, &token_a_mint_account), // size needed because syscall not stubbed
            token_a_program_id, // this should be system but we no-op the system program calls
        );
        let (token_a_fees_vault_key, _token_a_fees_vault_bump_seed) =
            seeds::pda::token_a_fees_vault_pda(&pool, &token_a_mint_key);
        let token_a_fees_vault_account = SolanaAccount::new(
            u32::MAX as u64,
            token::get_token_account_space(token_a_program_id, &token_a_mint_account), // size needed because syscall not stubbed
            token_a_program_id, // this should be system but we no-op the system program calls
        );
        let (admin_authority_token_a_ata_key, admin_authority_token_a_ata_account) =
            token::create_token_account(
                token_a_program_id,
                &token_a_mint_key,
                &mut token_a_mint_account,
                admin_authority,
                admin_authority,
                initial_supply_a,
            );

        let (token_b_mint_key, mut token_b_mint_account) = token::create_mint(
            token_b_program_id,
            admin_authority,
            None,
            None,
            &transfer_fees.token_b,
            token_b_decimals,
        );
        let (token_b_vault_key, _token_b_vault_bump_seed) =
            seeds::pda::token_b_vault_pda(&pool, &token_b_mint_key);
        let token_b_vault_account = SolanaAccount::new(
            u32::MAX as u64,
            token::get_token_account_space(token_b_program_id, &token_b_mint_account), // size needed because syscall not stubbed
            token_b_program_id, // this should be system but we no-op the system program calls
        );
        let (token_b_fees_vault_key, _token_b_fees_vault_bump_seed) =
            seeds::pda::token_b_fees_vault_pda(&pool, &token_b_mint_key);
        let token_b_fees_vault_account = SolanaAccount::new(
            u32::MAX as u64,
            token::get_token_account_space(token_b_program_id, &token_b_mint_account), // size needed because syscall not stubbed
            token_b_program_id, // this should be system but we no-op the system program calls
        );
        let (admin_authority_token_b_ata_key, admin_authority_token_b_ata_account) =
            token::create_token_account(
                token_b_program_id,
                &token_b_mint_key,
                &mut token_b_mint_account,
                admin_authority,
                admin_authority,
                initial_supply_b,
            );

        SwapAccountInfo {
            admin_authority: *admin_authority,
            pool_authority_bump_seed,
            pool_authority,
            fees,
            initial_supply,
            transfer_fees,
            pool,
            pool_account,
            swap_curve_key,
            swap_curve_account,
            swap_curve: SwapCurve::new_from_params(curve_params.clone()).unwrap(),
            curve_params,
            pool_token_mint_key,
            pool_token_mint_account,
            token_a_fees_vault_key,
            token_a_fees_vault_account,
            token_b_fees_vault_key,
            token_b_fees_vault_account,
            admin_authority_token_a_ata_key,
            admin_authority_token_a_ata_account,
            admin_authority_token_b_ata_key,
            admin_authority_token_b_ata_account,
            admin_authority_pool_token_ata_key,
            admin_authority_pool_token_ata_account,
            token_a_vault_key,
            token_a_vault_account,
            token_a_mint_key,
            token_a_mint_account,
            token_b_vault_key,
            token_b_vault_account,
            token_b_mint_key,
            token_b_mint_account,
            pool_token_program_id: *pool_token_program_id,
            token_a_program_id: *token_a_program_id,
            token_b_program_id: *token_b_program_id,
        }
    }

    pub fn initialize_pool(&mut self) -> ProgramResult {
        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);
        do_process_instruction(
            ix::initialize_pool(
                &crate::id(),
                &self.admin_authority,
                &self.pool,
                &self.swap_curve_key,
                &self.token_a_mint_key,
                &self.token_b_mint_key,
                &self.token_a_vault_key,
                &self.token_b_vault_key,
                &self.pool_authority,
                &self.pool_token_mint_key,
                &self.token_a_fees_vault_key,
                &self.token_b_fees_vault_key,
                &self.admin_authority_token_a_ata_key,
                &self.admin_authority_token_b_ata_key,
                &self.admin_authority_pool_token_ata_key,
                &self.pool_token_program_id,
                &self.token_a_program_id,
                &self.token_b_program_id,
                Initialize {
                    fees: self.fees,
                    initial_supply: self.initial_supply.clone(),
                    curve_parameters: self.curve_params.clone().into(),
                },
            )
            .unwrap(),
            vec![
                &mut SolanaAccount::default(),
                &mut self.pool_account,
                &mut self.swap_curve_account,
                &mut SolanaAccount::default(),
                &mut self.token_a_mint_account,
                &mut self.token_b_mint_account,
                &mut self.token_a_vault_account,
                &mut self.token_b_vault_account,
                &mut self.pool_token_mint_account,
                &mut self.token_a_fees_vault_account,
                &mut self.token_b_fees_vault_account,
                &mut self.admin_authority_token_a_ata_account,
                &mut self.admin_authority_token_b_ata_account,
                &mut self.admin_authority_pool_token_ata_account,
                &mut exe.clone(), // system_program
                &mut create_account_for_test(&Rent::default()),
                &mut exe.clone(), // pool_token_program
                &mut exe.clone(), // token_a_program
                &mut exe.clone(), // token_b_program
            ],
        )
    }

    pub fn setup_token_accounts(
        &mut self,
        mint_owner: &Pubkey,
        account_owner: &Pubkey,
        a_amount: u64,
        b_amount: u64,
        pool_amount: u64,
    ) -> (
        Pubkey,
        SolanaAccount,
        Pubkey,
        SolanaAccount,
        Pubkey,
        SolanaAccount,
    ) {
        let (token_a_key, token_a_account) = token::create_token_account(
            &self.token_a_program_id,
            &self.token_a_mint_key,
            &mut self.token_a_mint_account,
            mint_owner,
            account_owner,
            a_amount,
        );
        let (token_b_key, token_b_account) = token::create_token_account(
            &self.token_b_program_id,
            &self.token_b_mint_key,
            &mut self.token_b_mint_account,
            mint_owner,
            account_owner,
            b_amount,
        );
        let (pool_key, pool_account) = token::create_token_account(
            &self.pool_token_program_id,
            &self.pool_token_mint_key,
            &mut self.pool_token_mint_account,
            &self.pool_authority,
            account_owner,
            pool_amount,
        );
        (
            token_a_key,
            token_a_account,
            token_b_key,
            token_b_account,
            pool_key,
            pool_account,
        )
    }

    fn get_token_program_id(&self, account_key: &Pubkey) -> &Pubkey {
        if *account_key == self.token_a_vault_key {
            &self.token_a_program_id
        } else if *account_key == self.token_b_vault_key {
            &self.token_b_program_id
        } else {
            panic!("Could not find matching swap token account");
        }
    }

    fn get_token_mint(&self, account_key: &Pubkey) -> (Pubkey, SolanaAccount) {
        if *account_key == self.token_a_vault_key {
            (self.token_a_mint_key, self.token_a_mint_account.clone())
        } else if *account_key == self.token_b_vault_key {
            (self.token_b_mint_key, self.token_b_mint_account.clone())
        } else {
            panic!("Could not find matching swap token account");
        }
    }

    pub fn get_vault_account(&self, account_key: &Pubkey) -> &SolanaAccount {
        if account_key == &self.token_a_vault_key {
            &self.token_a_vault_account
        } else if account_key == &self.token_b_vault_key {
            &self.token_b_vault_account
        } else if account_key == &self.token_a_fees_vault_key {
            &self.token_a_fees_vault_account
        } else if account_key == &self.token_b_fees_vault_key {
            &self.token_b_fees_vault_account
        } else {
            panic!("Could not find matching swap token account");
        }
    }

    fn set_token_account(&mut self, account_key: &Pubkey, account: SolanaAccount) {
        if account_key == &self.token_a_vault_key {
            self.token_a_vault_account = account;
        } else if account_key == &self.token_b_vault_key {
            self.token_b_vault_account = account;
        } else if account_key == &self.token_a_fees_vault_key {
            self.token_a_fees_vault_account = account;
        } else if account_key == &self.token_b_fees_vault_key {
            self.token_b_fees_vault_account = account;
        } else {
            panic!("Could not find matching swap token account");
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn swap(
        &mut self,
        user_key: &Pubkey,
        user_source_key: &Pubkey,
        user_source_account: &mut SolanaAccount,
        source_vault_key: &Pubkey,
        source_fees_vault_key: &Pubkey,
        destination_vault_key: &Pubkey,
        user_destination_key: &Pubkey,
        user_destination_account: &mut SolanaAccount,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> ProgramResult {
        let user_transfer_key = Pubkey::new_unique();
        let source_token_program_id = self.get_token_program_id(source_vault_key);
        let destination_token_program_id = self.get_token_program_id(destination_vault_key);
        // approve moving from user source account
        do_process_instruction(
            approve(
                source_token_program_id,
                user_source_key,
                &user_transfer_key,
                user_key,
                &[],
                amount_in,
            )
            .unwrap(),
            vec![
                user_source_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        let (source_mint_key, mut source_mint_account) = self.get_token_mint(source_vault_key);
        let (destination_mint_key, mut destination_mint_account) =
            self.get_token_mint(destination_vault_key);
        let mut source_vault_account = self.get_vault_account(source_vault_key).clone();
        let mut destination_vault_account = self.get_vault_account(destination_vault_key).clone();
        let mut source_fees_vault_account = self.get_vault_account(source_fees_vault_key).clone();

        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);

        // perform the swap
        do_process_instruction(
            ix::swap(
                &crate::id(),
                &user_transfer_key,
                &self.pool,
                &self.swap_curve_key,
                &self.pool_authority,
                &source_mint_key,
                &destination_mint_key,
                source_vault_key,
                destination_vault_key,
                source_fees_vault_key,
                user_source_key,
                user_destination_key,
                None,
                source_token_program_id,
                destination_token_program_id,
                ix::Swap {
                    amount_in,
                    minimum_amount_out,
                },
            )
            .unwrap(),
            vec![
                &mut SolanaAccount::default(),
                &mut self.pool_account,
                &mut self.swap_curve_account,
                &mut SolanaAccount::default(),
                &mut source_mint_account,
                &mut destination_mint_account,
                &mut source_vault_account,
                &mut destination_vault_account,
                &mut source_fees_vault_account,
                user_source_account,
                user_destination_account,
                &mut exe.clone(), // Optional front end host fees - passed as the program if not present
                &mut exe.clone(), // source_token_program
                &mut exe.clone(), // destination_token_program
            ],
        )?;

        self.set_token_account(source_vault_key, source_vault_account);
        self.set_token_account(source_fees_vault_key, source_fees_vault_account);
        self.set_token_account(destination_vault_key, destination_vault_account);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn deposit(
        &mut self,
        depositor_key: &Pubkey,
        depositor_token_a_key: &Pubkey,
        depositor_token_a_account: &mut SolanaAccount,
        depositor_token_b_key: &Pubkey,
        depositor_token_b_account: &mut SolanaAccount,
        depositor_pool_key: &Pubkey,
        depositor_pool_account: &mut SolanaAccount,
        pool_token_amount: u64,
        maximum_token_a_amount: u64,
        maximum_token_b_amount: u64,
    ) -> ProgramResult {
        let user_transfer_authority = Pubkey::new_unique();
        let token_a_program_id = depositor_token_a_account.owner;
        do_process_instruction(
            approve(
                &token_a_program_id,
                depositor_token_a_key,
                &user_transfer_authority,
                depositor_key,
                &[],
                maximum_token_a_amount,
            )
            .unwrap(),
            vec![
                depositor_token_a_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        let token_b_program_id = depositor_token_b_account.owner;
        do_process_instruction(
            approve(
                &token_b_program_id,
                depositor_token_b_key,
                &user_transfer_authority,
                depositor_key,
                &[],
                maximum_token_b_amount,
            )
            .unwrap(),
            vec![
                depositor_token_b_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        let pool_token_program_id = depositor_pool_account.owner;

        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);

        do_process_instruction(
            ix::deposit(
                &crate::id(),
                &user_transfer_authority,
                &self.pool,
                &self.swap_curve_key,
                &self.pool_authority,
                &self.token_a_mint_key,
                &self.token_b_mint_key,
                &self.token_a_vault_key,
                &self.token_b_vault_key,
                &self.pool_token_mint_key,
                depositor_token_a_key,
                depositor_token_b_key,
                depositor_pool_key,
                &pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
                ix::Deposit {
                    pool_token_amount,
                    maximum_token_a_amount,
                    maximum_token_b_amount,
                },
            )
            .unwrap(),
            vec![
                &mut SolanaAccount::default(),
                &mut self.pool_account,
                &mut self.swap_curve_account,
                &mut SolanaAccount::default(),
                &mut self.token_a_mint_account,
                &mut self.token_b_mint_account,
                &mut self.token_a_vault_account,
                &mut self.token_b_vault_account,
                &mut self.pool_token_mint_account,
                depositor_token_a_account,
                depositor_token_b_account,
                depositor_pool_account,
                &mut exe.clone(),
                &mut exe.clone(),
                &mut exe.clone(),
            ],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn withdraw(
        &mut self,
        user_key: &Pubkey,
        user_pool_token_key: &Pubkey,
        user_pool_token_account: &mut SolanaAccount,
        user_token_a_key: &Pubkey,
        user_token_a_account: &mut SolanaAccount,
        user_token_b_key: &Pubkey,
        user_token_b_account: &mut SolanaAccount,
        pool_token_amount: u64,
        minimum_token_a_amount: u64,
        minimum_token_b_amount: u64,
    ) -> ProgramResult {
        let pool_token_program_id = user_pool_token_account.owner;
        let user_transfer_authority_key = Pubkey::new_unique();
        // approve user transfer authority to take out pool tokens
        do_process_instruction(
            approve(
                &pool_token_program_id,
                user_pool_token_key,
                &user_transfer_authority_key,
                user_key,
                &[],
                pool_token_amount,
            )
            .unwrap(),
            vec![
                user_pool_token_account,
                &mut SolanaAccount::default(),
                &mut SolanaAccount::default(),
            ],
        )
        .unwrap();

        // withdraw token a and b correctly
        let token_a_program_id = user_token_a_account.owner;
        let token_b_program_id = user_token_b_account.owner;

        let exe = &mut SolanaAccount::default();
        exe.set_executable(true);

        do_process_instruction(
            ix::withdraw(
                &crate::id(),
                &user_transfer_authority_key,
                &self.pool,
                &self.swap_curve_key,
                &self.pool_authority,
                &self.token_a_mint_key,
                &self.token_b_mint_key,
                &self.token_a_vault_key,
                &self.token_b_vault_key,
                &self.pool_token_mint_key,
                &self.token_a_fees_vault_key,
                &self.token_b_fees_vault_key,
                user_token_a_key,
                user_token_b_key,
                user_pool_token_key,
                &pool_token_program_id,
                &token_a_program_id,
                &token_b_program_id,
                ix::Withdraw {
                    pool_token_amount,
                    minimum_token_a_amount,
                    minimum_token_b_amount,
                },
            )
            .unwrap(),
            vec![
                &mut SolanaAccount::default(),
                &mut self.pool_account,
                &mut self.swap_curve_account,
                &mut SolanaAccount::default(),
                &mut self.token_a_mint_account,
                &mut self.token_b_mint_account,
                &mut self.token_a_vault_account,
                &mut self.token_b_vault_account,
                &mut self.pool_token_mint_account,
                &mut self.token_a_fees_vault_account,
                &mut self.token_b_fees_vault_account,
                user_token_a_account,
                user_token_b_account,
                user_pool_token_account,
                &mut exe.clone(), // pool_token_program
                &mut exe.clone(), // token_a_token_program
                &mut exe.clone(), // token_b_token_program
            ],
        )
    }
}

pub fn do_process_instruction_with_fee_constraints(
    instruction: Instruction,
    accounts: Vec<&mut SolanaAccount>,
    _swap_constraints: &Option<SwapConstraints>, // todo - elliot - compile time constraints
) -> ProgramResult {
    test_syscall_stubs();

    // approximate the logic in the actual runtime which runs the instruction
    // and only updates accounts if the instruction is successful
    let mut account_clones = accounts.iter().map(|x| (*x).clone()).collect::<Vec<_>>();
    let mut account_infos = instruction
        .accounts
        .iter()
        .zip(account_clones.iter_mut())
        .map(|(account_meta, account)| {
            AccountInfo::new(
                &account_meta.pubkey,
                account_meta.is_signer,
                account_meta.is_writable,
                &mut account.lamports,
                &mut account.data,
                &account.owner,
                account.executable,
                account.rent_epoch,
            )
        })
        .collect::<Vec<_>>();

    let res = if instruction.program_id == crate::id() {
        crate::entry(&instruction.program_id, &account_infos, &instruction.data)
    } else if instruction.program_id == spl_token::id() {
        spl_token::processor::Processor::process(
            &instruction.program_id,
            &account_infos,
            &instruction.data,
        )
    } else if instruction.program_id == spl_token_2022::id() {
        spl_token_2022::processor::Processor::process(
            &instruction.program_id,
            &account_infos,
            &instruction.data,
        )
    } else {
        Err(ProgramError::IncorrectProgramId)
    };

    if res.is_ok() {
        let mut account_metas = instruction
            .accounts
            .iter()
            .zip(accounts)
            .map(|(account_meta, account)| (&account_meta.pubkey, account))
            .collect::<Vec<_>>();
        for account_info in account_infos.iter() {
            for account_meta in account_metas.iter_mut() {
                if account_info.key == account_meta.0 {
                    let account = &mut account_meta.1;
                    account.owner = *account_info.owner;
                    account.lamports = **account_info.lamports.borrow();
                    account.data = account_info.data.borrow().to_vec();
                }
            }
        }
    }
    res
}

pub fn do_process_instruction(
    instruction: Instruction,
    accounts: Vec<&mut SolanaAccount>,
) -> ProgramResult {
    do_process_instruction_with_fee_constraints(instruction, accounts, &SWAP_CONSTRAINTS)
}
