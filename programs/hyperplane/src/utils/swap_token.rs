use crate::utils::seeds;
use anchor_lang::prelude::{AccountInfo, CpiContext, Result};

/// Issue an spl_token or spl_token_2022 `TransferChecked` instruction.
#[allow(clippy::too_many_arguments)]
pub fn transfer_from_vault<'info>(
    token_program: AccountInfo<'info>,
    pool: AccountInfo<'info>,
    source: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    destination: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    pool_authority_bump: u64,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    let inner_seeds = [
        seeds::POOL_AUTHORITY,
        pool.key.as_ref(),
        &[u8::try_from(pool_authority_bump).unwrap()],
    ];
    let signer_seeds = &[&inner_seeds[..]];

    anchor_spl::token_2022::transfer_checked(
        CpiContext::new_with_signer(
            token_program,
            anchor_spl::token_2022::TransferChecked {
                from: source,
                mint,
                to: destination,
                authority,
            },
            signer_seeds,
        ),
        amount,
        decimals,
    )?;

    Ok(())
}

/// Issue an spl_token or spl_token_2022 `TransferChecked` instruction.
#[allow(clippy::too_many_arguments)]
pub fn transfer_from_user<'info>(
    token_program: AccountInfo<'info>,
    source: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    destination: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    anchor_spl::token_2022::transfer_checked(
        CpiContext::new(
            token_program,
            anchor_spl::token_2022::TransferChecked {
                from: source,
                mint,
                to: destination,
                authority,
            },
        ),
        amount,
        decimals,
    )?;

    Ok(())
}
