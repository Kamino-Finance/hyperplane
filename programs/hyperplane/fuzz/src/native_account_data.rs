use solana_program::{account_info::AccountInfo, clock::Epoch, pubkey::Pubkey};

#[derive(Clone)]
pub struct NativeAccountData {
    pub key: Pubkey,
    pub lamports: u64,
    pub data: Vec<u8>,
    pub program_id: Pubkey,
    pub is_signer: bool,
    pub executable: bool,
}

impl NativeAccountData {
    pub fn new(size: usize, program_id: Pubkey) -> Self {
        Self::new_with_key(Pubkey::new_unique(), size, program_id)
    }

    pub fn new_with_key(key: Pubkey, size: usize, program_id: Pubkey) -> Self {
        Self {
            key,
            lamports: u32::MAX.into(),
            data: vec![0; size],
            program_id,
            is_signer: false,
            executable: false,
        }
    }

    pub fn new_from_account_info(account_info: &AccountInfo) -> Self {
        Self {
            key: *account_info.key,
            lamports: **account_info.lamports.borrow(),
            data: account_info.data.borrow().to_vec(),
            program_id: *account_info.owner,
            is_signer: account_info.is_signer,
            executable: account_info.executable,
        }
    }

    pub fn as_account_info(&mut self) -> AccountInfo {
        AccountInfo::new(
            &self.key,
            self.is_signer,
            true,
            &mut self.lamports,
            &mut self.data[..],
            &self.program_id,
            self.executable,
            Epoch::default(),
        )
    }
}
