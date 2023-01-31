use anchor_lang::prelude::*;
use anchor_lang::solana_program::entrypoint::{ProgramResult, SUCCESS};
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program_stubs;
use anchor_lang::solana_program::system_program;

struct TestSyscallStubs {}
impl program_stubs::SyscallStubs for TestSyscallStubs {
    fn sol_invoke_signed(
        &self,
        instruction: &Instruction,
        account_infos: &[AccountInfo],
        signers_seeds: &[&[&[u8]]],
    ) -> ProgramResult {
        let mut account_infos_ordered = vec![];

        msg!("TestSyscallStubs::sol_invoke_signed()");

        // order account infos as the instruction expects them as defined in the account_metas
        // re-add signer flag if signer
        for meta in instruction.accounts.iter() {
            for account_info in account_infos.iter() {
                if meta.pubkey == *account_info.key {
                    let mut new_account_info = account_info.clone();
                    for seeds in signers_seeds.iter() {
                        msg!("TestSyscallStubs::sol_invoke_signed() seeds: {:?}", seeds);
                        let signer = Pubkey::create_program_address(seeds, &crate::id()).unwrap();
                        if *account_info.key == signer {
                            new_account_info.is_signer = true;
                        }
                    }
                    account_infos_ordered.push(new_account_info);
                }
            }
        }

        if instruction.program_id == spl_token::id() {
            msg!("sol_invoke_signed: token program id");
            spl_token::processor::Processor::process(
                &instruction.program_id,
                &account_infos_ordered,
                &instruction.data,
            )?; // NOTE: unwrap here to get a stack trace
        } else if instruction.program_id == spl_token_2022::id() {
            msg!("sol_invoke_signed: token 2022 program id");
            spl_token_2022::processor::Processor::process(
                &instruction.program_id,
                &account_infos_ordered,
                &instruction.data,
            )?; // NOTE: unwrap here to get a stack trace
        } else if instruction.program_id == system_program::id() {
            // https://github.com/solana-labs/solana/blob/master/runtime/src/system_instruction_processor.rs
            // we have the system program defined in the master/runtime of the main repo
            msg!("sol_invoke_signed: system program id");
            msg!("ix: {:?}", instruction);
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

pub fn test_syscall_stubs() {
    use std::sync::Once;
    static ONCE: Once = Once::new();

    ONCE.call_once(|| {
        program_stubs::set_syscall_stubs(Box::new(TestSyscallStubs {}));
    });
}
