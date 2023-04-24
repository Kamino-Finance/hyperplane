pub const POOL_AUTHORITY: &[u8] = b"pauthority";
pub const POOL_TOKEN_MINT: &[u8] = b"lp";
pub const SWAP_CURVE: &[u8] = b"curve";
pub const TOKEN_A_VAULT: &[u8] = b"pvault_a";
pub const TOKEN_B_VAULT: &[u8] = b"pvault_b";
pub const TOKEN_A_FEES_VAULT: &[u8] = b"fvault_a";
pub const TOKEN_B_FEES_VAULT: &[u8] = b"fvault_b";

pub mod pda {
    use anchor_lang::prelude::Pubkey;

    use super::*;
    use crate::ID;

    pub struct InitPoolPdas {
        pub curve: Pubkey,
        pub authority: Pubkey,
        pub token_a_vault: Pubkey,
        pub token_b_vault: Pubkey,
        pub pool_token_mint: Pubkey,
        pub token_a_fees_vault: Pubkey,
        pub token_b_fees_vault: Pubkey,
    }

    pub fn pool_authority_pda(pool: &Pubkey) -> (Pubkey, u8) {
        pool_authority_pda_program_id(&ID, pool)
    }

    pub fn pool_authority_pda_program_id(program_id: &Pubkey, pool: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[POOL_AUTHORITY, pool.as_ref()], program_id)
    }

    pub fn token_a_vault_pda(pool: &Pubkey, token_a_mint: &Pubkey) -> (Pubkey, u8) {
        token_a_vault_pda_program_id(&ID, pool, token_a_mint)
    }

    pub fn token_a_vault_pda_program_id(
        program_id: &Pubkey,
        pool: &Pubkey,
        token_a_mint: &Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[TOKEN_A_VAULT, pool.as_ref(), token_a_mint.as_ref()],
            program_id,
        )
    }

    pub fn token_b_vault_pda(pool: &Pubkey, token_b_mint: &Pubkey) -> (Pubkey, u8) {
        token_b_vault_pda_program_id(&ID, pool, token_b_mint)
    }

    pub fn token_b_vault_pda_program_id(
        program_id: &Pubkey,
        pool: &Pubkey,
        token_b_mint: &Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[TOKEN_B_VAULT, pool.as_ref(), token_b_mint.as_ref()],
            program_id,
        )
    }

    pub fn token_a_fees_vault_pda(pool: &Pubkey, token_a_mint: &Pubkey) -> (Pubkey, u8) {
        token_a_fees_vault_pda_program_id(&ID, pool, token_a_mint)
    }

    pub fn token_a_fees_vault_pda_program_id(
        program_id: &Pubkey,
        pool: &Pubkey,
        token_a_mint: &Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[TOKEN_A_FEES_VAULT, pool.as_ref(), token_a_mint.as_ref()],
            program_id,
        )
    }

    pub fn token_b_fees_vault_pda(pool: &Pubkey, token_b_mint: &Pubkey) -> (Pubkey, u8) {
        token_b_fees_vault_pda_program_id(&ID, pool, token_b_mint)
    }

    pub fn token_b_fees_vault_pda_program_id(
        program_id: &Pubkey,
        pool: &Pubkey,
        token_b_mint: &Pubkey,
    ) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[TOKEN_B_FEES_VAULT, pool.as_ref(), token_b_mint.as_ref()],
            program_id,
        )
    }

    pub fn init_pool_pdas(
        pool: &Pubkey,
        token_a_mint: &Pubkey,
        token_b_mint: &Pubkey,
    ) -> InitPoolPdas {
        init_pool_pdas_program_id(&ID, pool, token_a_mint, token_b_mint)
    }

    pub fn init_pool_pdas_program_id(
        program_id: &Pubkey,
        pool: &Pubkey,
        token_a_mint: &Pubkey,
        token_b_mint: &Pubkey,
    ) -> InitPoolPdas {
        let (curve, _swap_curve_bump_seed) =
            Pubkey::find_program_address(&[SWAP_CURVE, pool.as_ref()], program_id);

        let (authority, _pool_authority_bump) = pool_authority_pda_program_id(program_id, pool);

        let (token_a_vault, _token_a_vault_bump_seed) =
            token_a_vault_pda_program_id(program_id, pool, token_a_mint);
        let (token_b_vault, _token_b_vault_bump_seed) =
            token_b_vault_pda_program_id(program_id, pool, token_b_mint);

        let (pool_token_mint, _pool_token_mint_bump_seed) =
            Pubkey::find_program_address(&[POOL_TOKEN_MINT, pool.as_ref()], program_id);

        let (token_a_fees_vault, _token_a_fees_vault_bump_seed) =
            token_a_fees_vault_pda_program_id(program_id, pool, token_a_mint);
        let (token_b_fees_vault, _token_b_fees_vault_bump_seed) =
            token_b_fees_vault_pda_program_id(program_id, pool, token_b_mint);

        InitPoolPdas {
            curve,
            authority,
            token_a_vault,
            token_b_vault,
            pool_token_mint,
            token_a_fees_vault,
            token_b_fees_vault,
        }
    }
}
