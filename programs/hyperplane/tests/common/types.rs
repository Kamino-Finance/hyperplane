use std::sync::Arc;

use anchor_lang::prelude::{thiserror, Pubkey, Rent};
use anchor_spl::{token::spl_token, token_2022::spl_token_2022};
use derive_more::Constructor;
use solana_program_test::ProgramTestContext;
use solana_sdk::{signature::Keypair, signer::Signer};
use thiserror::Error;

// --- GENERIC TYPES ---

pub struct TestContext {
    pub context: ProgramTestContext,
    pub rent: Rent,
}

#[derive(PartialEq, Eq, Error, Debug)]
pub enum TestError {
    #[error("Insufficient collateral to cover debt")]
    CannotDeserialize,
    #[error("Wrong discriminator")]
    BadDiscriminator,
    #[error("Account not found")]
    AccountNotFound,
    #[error("Unknown Error")]
    UnknownError,
}

// ---- POOL TYPES ----

#[derive(Clone, Debug)]
pub struct SwapPoolAccounts {
    pub admin: PoolAdminAccounts,
    pub pool: Arc<Keypair>,
    pub curve: Pubkey,
    pub authority: Pubkey,
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub pool_token_mint: Pubkey,
    pub token_a_vault: Pubkey,
    pub token_b_vault: Pubkey,
    pub token_a_fees_vault: Pubkey,
    pub token_b_fees_vault: Pubkey,
    pub token_a_token_program: Pubkey,
    pub token_b_token_program: Pubkey,
    pub pool_token_program: Pubkey,
}

impl SwapPoolAccounts {
    pub fn pubkey(&self) -> Pubkey {
        self.pool.pubkey()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TokenSpec {
    pub decimals: u8,
    pub transfer_fee_bps: u16,
    pub token_program: Pubkey,
}

impl Default for TokenSpec {
    fn default() -> Self {
        Self::new(6, 0, spl_token::id())
    }
}

impl TokenSpec {
    pub fn new(decimals: u8, transfer_fee_bps: u16, token_program: Pubkey) -> Self {
        if transfer_fee_bps > 0 && token_program != spl_token_2022::id() {
            panic!("Transfer fees are only supported for spl-token-2022");
        }
        Self {
            decimals,
            transfer_fee_bps,
            token_program,
        }
    }
    pub fn spl_token(decimals: u8) -> Self {
        Self::new(decimals, 0, spl_token::id())
    }
    pub fn transfer_fees(bps: u16) -> Self {
        Self::new(6, bps, spl_token_2022::id())
    }
}

#[derive(Clone, Copy, Debug, Default, Constructor)]
pub struct SwapPairSpec {
    pub a: TokenSpec,
    pub b: TokenSpec,
}

impl SwapPairSpec {
    pub fn spl_tokens(a_decimals: u8, b_decimals: u8) -> Self {
        Self::new(
            TokenSpec::spl_token(a_decimals),
            TokenSpec::spl_token(b_decimals),
        )
    }
}

// ---- USER TYPES ----

#[derive(Clone, Debug, Constructor)]
pub struct PoolAdminAccounts {
    pub admin: Arc<Keypair>,
    pub token_a_ata: Pubkey,
    pub token_b_ata: Pubkey,
    pub pool_token_ata: Arc<Keypair>,
}

impl PoolAdminAccounts {
    pub fn pubkey(&self) -> Pubkey {
        self.admin.pubkey()
    }
}

#[derive(Clone, Debug, Constructor)]
pub struct PoolUserAccounts {
    pub user: Arc<Keypair>,
    pub token_a_ata: Pubkey,
    pub token_b_ata: Pubkey,
    pub pool_token_ata: Pubkey,
}

impl PoolUserAccounts {
    pub fn pubkey(&self) -> Pubkey {
        self.user.pubkey()
    }
}

impl From<PoolAdminAccounts> for PoolUserAccounts {
    fn from(admin: PoolAdminAccounts) -> Self {
        Self {
            user: admin.admin,
            token_a_ata: admin.token_a_ata,
            token_b_ata: admin.token_b_ata,
            pool_token_ata: admin.pool_token_ata.pubkey(),
        }
    }
}
