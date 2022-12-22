use crate::native_account_data::NativeAccountData;

use solana_program::clock::Clock;
use solana_program::entrypoint::SUCCESS;
use solana_program::rent::Rent;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, instruction::Instruction, program_stubs,
    pubkey::Pubkey,
};

struct TestSyscallStubs {}
impl program_stubs::SyscallStubs for TestSyscallStubs {
    fn sol_invoke_signed(
        &self,
        instruction: &Instruction,
        account_infos: &[AccountInfo],
        signers_seeds: &[&[&[u8]]],
    ) -> ProgramResult {
        let mut account_infos_ordered = vec![];

        for meta in instruction.accounts.iter() {
            for account_info in account_infos.iter() {
                if meta.pubkey == *account_info.key {
                    let mut new_account_info = account_info.clone();
                    for seeds in signers_seeds.iter() {
                        let signer =
                            Pubkey::create_program_address(seeds, &hyperplane::id()).unwrap();
                        if *account_info.key == signer {
                            new_account_info.is_signer = true;
                        }
                    }
                    account_infos_ordered.push(new_account_info);
                }
            }
        }

        if instruction.program_id == spl_token::id() {
            spl_token::processor::Processor::process(
                &instruction.program_id,
                &account_infos_ordered,
                &instruction.data,
            )?; // NOTE: unwrap here to get a stack trace
        } else if instruction.program_id == spl_token_2022::id() {
            spl_token_2022::processor::Processor::process(
                &instruction.program_id,
                &account_infos_ordered,
                &instruction.data,
            )?; // NOTE: unwrap here to get a stack trace
        } else if instruction.program_id == solana_program::system_program::id() {
            // https://github.com/solana-labs/solana/blob/master/runtime/src/system_instruction_processor.rs
            // we have the system program defined in the master/runtime of the main repo
        } else {
            unreachable!("sol_invoke_signed: unhandled program_id");
        }

        Ok(())
    }

    fn sol_get_clock_sysvar(&self, var_addr: *mut u8) -> u64 {
        unsafe {
            *(var_addr as *mut _ as *mut Clock) = Clock::default();
        }
        SUCCESS
    }

    fn sol_get_rent_sysvar(&self, var_addr: *mut u8) -> u64 {
        unsafe {
            *(var_addr as *mut _ as *mut Rent) = Rent::default();
        }
        SUCCESS
    }
}

fn test_syscall_stubs() {
    use std::sync::Once;
    static ONCE: Once = Once::new();

    ONCE.call_once(|| {
        program_stubs::set_syscall_stubs(Box::new(TestSyscallStubs {}));
    });
}

pub fn do_process_instruction(instruction: Instruction, accounts: &[AccountInfo]) -> ProgramResult {
    test_syscall_stubs();

    // approximate the logic in the actual runtime which runs the instruction
    // and only updates accounts if the instruction is successful
    let mut account_data = accounts
        .iter()
        .map(NativeAccountData::new_from_account_info)
        .collect::<Vec<_>>();
    let account_infos = account_data
        .iter_mut()
        .map(NativeAccountData::as_account_info)
        .zip(instruction.accounts.iter())
        .map(|(mut account_info, meta)| {
            account_info.is_signer = meta.is_signer;
            account_info.is_writable = meta.is_writable;
            account_info
        })
        .collect::<Vec<_>>();

    let res = if instruction.program_id == hyperplane::id() {
        hyperplane::entry(&instruction.program_id, &account_infos, &instruction.data)
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
        unreachable!(
            "sol_invoke_signed: unhandled program_id {}",
            instruction.program_id
        );
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
                    let mut lamports = account.lamports.borrow_mut();
                    **lamports = **account_info.lamports.borrow();
                    let mut data = account.data.borrow_mut();
                    data.clone_from_slice(&account_info.data.borrow());
                }
            }
        }
    } else if instruction.program_id != hyperplane::id() {
        println!("Token program {} error: {:?}", instruction.program_id, res);
    }
    res
}
