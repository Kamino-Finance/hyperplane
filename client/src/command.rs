use std::{path::PathBuf, str::FromStr};

use anchor_client::{
    anchor_lang::prelude::Pubkey,
    solana_sdk::{
        program_pack::Pack,
        signature::{Keypair, Signer},
    },
};
use anyhow::Result;
use hyperplane::{
    curve::{base::CurveType, calculator::CurveCalculator},
    ix::Initialize,
    state::{
        ConstantPriceCurve, ConstantProductCurve, OffsetCurve, StableCurve, SwapPool,
        UpdatePoolConfigMode,
    },
};
use orbit_link::async_client::AsyncClient;
use spl_associated_token_account as ata;
use spl_token::state::Mint;
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::info;

use crate::{
    client::HyperplaneClient, configs::PoolConfigValue, model::InitializePoolConfig, send_tx,
};

pub async fn create_ata<T: AsyncClient, S: Signer>(
    hyperplane: &HyperplaneClient<T, S>,
    owner: Pubkey,
    mint: Pubkey,
) -> Result<()> {
    use spl_associated_token_account::instruction;

    let address = ata::get_associated_token_address(&owner, &mint);

    let builder =
        hyperplane
            .client
            .tx_builder()
            .add_ix(instruction::create_associated_token_account(
                &hyperplane.client.payer().unwrap().pubkey(),
                &owner,
                &mint,
                &spl_token::id(),
            ));

    send_tx!(hyperplane, builder, []);

    info!(
        "Created ATA {} for owner {} for mint {}",
        address, owner, mint
    );

    Ok(())
}

pub async fn create_mint<T: AsyncClient, S: Signer>(
    hyperplane: &HyperplaneClient<T, S>,
    out: PathBuf,
    mint_authority: Pubkey,
    initial_supply: Option<u64>,
) -> Result<()> {
    let mint = Keypair::new();
    let decimals = 6;

    let mut builder = hyperplane
        .client
        .tx_builder()
        .add_ix(
            hyperplane
                .client
                .create_account_ix(&mint.pubkey(), Mint::LEN, &spl_token::id())
                .await?,
        )
        .add_ix(
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &mint.pubkey(),
                &mint_authority,
                None,
                decimals,
            )
            .unwrap(),
        );
    if let Some(n) = initial_supply {
        if n > 0 {
            let ata = ata::get_associated_token_address(&mint_authority, &mint.pubkey());
            builder = builder
                .add_ix(
                    spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                        &hyperplane.client.payer().unwrap().pubkey(),
                        &mint_authority,
                        &mint.pubkey(),
                        &spl_token::id(),
                    )
                ).add_ix(
                spl_token::instruction::mint_to(
                    &spl_token::id(),
                    &mint.pubkey(),
                    &ata,
                    &mint_authority,
                    &[&mint_authority],
                    n
                ).unwrap()
            );
            info!(
                "Minting {} tokens to ATA {} for owner {}.",
                initial_supply.unwrap(),
                mint_authority,
                ata
            );
        }
    }

    send_tx!(hyperplane, builder, [&mint]);

    let mut file = File::create(&out).await?;
    file.write_all(format!("{:?}", mint.to_bytes()).as_bytes())
        .await?;

    info!(
        "Created mint {} and wrote to {}.",
        mint.pubkey(),
        out.to_string_lossy()
    );

    Ok(())
}

pub async fn initialize_pool<T: AsyncClient, S: Signer>(
    hyperplane: &HyperplaneClient<T, S>,
    admin: Pubkey,
    config: PathBuf,
    admin_token_a_ata: Option<Pubkey>,
    admin_token_b_ata: Option<Pubkey>,
) -> Result<()> {
    let config: InitializePoolConfig =
        serde_json::from_reader(File::open(config).await?.into_std().await)?;
    let token_a_mint = Pubkey::from_str(&config.token_a_mint)?;
    let token_b_mint = Pubkey::from_str(&config.token_b_mint)?;

    let admin_token_a_ata = admin_token_a_ata
        .unwrap_or_else(|| ata::get_associated_token_address(&admin, &token_a_mint));
    let admin_token_b_ata = admin_token_b_ata
        .unwrap_or_else(|| ata::get_associated_token_address(&admin, &token_b_mint));

    hyperplane
        .initialize_pool(
            admin,
            admin_token_a_ata,
            admin_token_b_ata,
            Initialize {
                fees: config.fees,
                curve_parameters: config.curve,
                initial_supply: config.initial_supply,
            },
        )
        .await?;
    Ok(())
}

pub async fn update_pool<T: AsyncClient, S: Signer>(
    hyperplane: &HyperplaneClient<T, S>,
    admin: Pubkey,
    pool: Pubkey,
    mode: UpdatePoolConfigMode,
    value: String,
) -> Result<()> {
    let update = PoolConfigValue::new_from_str(mode, value);
    hyperplane
        .update_pool_config(admin, pool, update.into())
        .await?;
    Ok(())
}

pub async fn print_pool<T: AsyncClient, S: Signer>(
    hyperplane: &HyperplaneClient<T, S>,
    pool_pubkey: Pubkey,
) -> Result<()> {
    let pool: SwapPool = hyperplane.client.get_anchor_account(&pool_pubkey).await?;
    let curve: Box<dyn CurveCalculator> = match CurveType::try_from(pool.curve_type).unwrap() {
        CurveType::ConstantProduct => Box::new(
            hyperplane
                .client
                .get_anchor_account::<ConstantProductCurve>(&pool.swap_curve)
                .await?,
        ),
        CurveType::ConstantPrice => Box::new(
            hyperplane
                .client
                .get_anchor_account::<ConstantPriceCurve>(&pool.swap_curve)
                .await?,
        ),
        CurveType::Stable => Box::new(
            hyperplane
                .client
                .get_anchor_account::<StableCurve>(&pool.swap_curve)
                .await?,
        ),
        CurveType::Offset => Box::new(
            hyperplane
                .client
                .get_anchor_account::<OffsetCurve>(&pool.swap_curve)
                .await?,
        ),
    };
    info!("\x1b[32mPool {}:\x1b\n\n{:#?}\n\n", pool_pubkey, pool);
    info!("\x1b[32mCurve {}:\x1b\n\n{:#?}\n\n", pool.swap_curve, curve);
    Ok(())
}
