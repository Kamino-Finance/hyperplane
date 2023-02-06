use anchor_lang::prelude::AccountInfo;
use anchor_lang::prelude::CpiContext;
use anchor_lang::Result;

/// Issue an spl_token or spl_token_2022 `Mint` instruction.
pub fn mint<'info>(
    token_program: AccountInfo<'info>,
    pool: AccountInfo<'info>,
    pool_token_mint: AccountInfo<'info>,
    pool_authority: AccountInfo<'info>,
    pool_authority_bump: u64,
    user_pool_token_ata: AccountInfo<'info>,
    shares_to_mint: u64,
) -> Result<()> {
    let inner_seeds = [
        b"pauthority".as_ref(),
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
        shares_to_mint,
    )?;

    Ok(())
}

pub fn burn<'info>(
    shares_mint: AccountInfo<'info>,
    user_shares_ata: AccountInfo<'info>,
    user: AccountInfo<'info>,
    token_program: AccountInfo<'info>,
    shares_to_burn: u64,
) -> Result<()> {
    anchor_spl::token::burn(
        CpiContext::new(
            token_program,
            anchor_spl::token::Burn {
                mint: shares_mint,
                from: user_shares_ata,
                authority: user,
            },
        ),
        shares_to_burn,
    )?;

    Ok(())
}
