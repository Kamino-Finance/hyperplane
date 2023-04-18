use std::sync::Arc;

use anchor_lang::prelude::{thiserror, Pubkey, Rent};
use anchor_spl::token::spl_token;
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
pub struct TradingTokenSpec {
    pub a_decimals: u8,
    pub b_decimals: u8,
    pub a_token_program: Pubkey,
    pub b_token_program: Pubkey,
}

impl Default for TradingTokenSpec {
    fn default() -> Self {
        Self {
            a_decimals: 6,
            b_decimals: 6,
            a_token_program: spl_token::id(),
            b_token_program: spl_token::id(),
        }
    }
}

impl TradingTokenSpec {
    pub fn new_spl_token(a_decimals: u8, b_decimals: u8) -> Self {
        Self {
            a_decimals,
            b_decimals,
            a_token_program: spl_token::id(),
            b_token_program: spl_token::id(),
        }
    }
}

// ---- USER TYPES ----

#[derive(Clone, Debug)]
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

impl PoolAdminAccounts {
    pub fn new(
        admin: Arc<Keypair>,
        token_a_ata: Pubkey,
        token_b_ata: Pubkey,
        pool_token_ata: Arc<Keypair>,
    ) -> Self {
        Self {
            admin,
            token_a_ata,
            token_b_ata,
            pool_token_ata,
        }
    }
}

#[derive(Clone, Debug)]
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

impl PoolUserAccounts {
    pub fn new(
        user: Arc<Keypair>,
        token_a_ata: Pubkey,
        token_b_ata: Pubkey,
        pool_token_ata: Pubkey,
    ) -> Self {
        Self {
            user,
            token_a_ata,
            token_b_ata,
            pool_token_ata,
        }
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

pub enum AorB {
    A,
    B,
}
