use anchor_lang::{
    prelude::{AccountInfo, CpiContext},
    Result,
};

use crate::utils::seeds;

/// Issue an spl_token or spl_token_2022 `Mint` instruction.
pub fn mint<'info>(
    token_program: AccountInfo<'info>,
    pool: AccountInfo<'info>,
    pool_token_mint: AccountInfo<'info>,
    pool_authority: AccountInfo<'info>,
    pool_authority_bump: u64,
    user_pool_token_ata: AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    let inner_seeds = [
        seeds::POOL_AUTHORITY,
        pool.key.as_ref(),
        &[u8::try_from(pool_authority_bump).unwrap()],
    ];
    let signer_seeds = &[&inner_seeds[..]];

    anchor_spl::token_2022::mint_to(
        CpiContext::new_with_signer(
            token_program,
            anchor_spl::token_2022::MintTo {
                mint: pool_token_mint,
                to: user_pool_token_ata,
                authority: pool_authority,
            },
            signer_seeds,
        ),
        amount,
    )?;

    Ok(())
}

pub fn burn<'info>(
    pool_token_mint: AccountInfo<'info>,
    user_pool_token_ata: AccountInfo<'info>,
    user: AccountInfo<'info>,
    token_program: AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    anchor_spl::token_2022::burn(
        CpiContext::new(
            token_program,
            anchor_spl::token_2022::Burn {
                mint: pool_token_mint,
                from: user_pool_token_ata,
                authority: user,
            },
        ),
        amount,
    )?;

    Ok(())
}
