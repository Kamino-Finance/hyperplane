use std::path::PathBuf;

use anchor_client::{
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig, pubkey::Pubkey, signature::read_keypair_file,
        signer::Signer,
    },
    Cluster,
};
use anyhow::Result;
use clap::{Parser, Subcommand};
use hyperplane::state::UpdatePoolConfigMode;
use hyperplane_client::{
    client::{Config, HyperplaneClient},
    command,
};
use orbit_link::OrbitLink;
use tracing::info;

static PROGRAM_ID: Pubkey = hyperplane::ID;

#[derive(Parser, Debug, PartialEq)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Subcommand to execute
    #[clap(subcommand)]
    action: Actions,

    /// Connect to solana validator
    #[clap(short, long, env, parse(try_from_str), default_value = "localnet")]
    url: Cluster,

    /// Program Id
    #[clap(short, long, env, default_value_t = PROGRAM_ID)]
    program: Pubkey,

    /// Account keypair to pay for the transactions
    /// Defaults to Keypair::new() which can be useful for dry-run
    #[clap(short, long, env, parse(from_os_str))]
    keypair: PathBuf,

    /// Account to sign transactions, i.e. the pool admin, token account owner
    /// Useful for multisig, if not specified, defaults to -k argument
    #[clap(
        short,
        long,
        env,
        parse(try_from_str),
        alias = "admin",
        alias = "owner"
    )]
    signer: Option<Pubkey>,

    /// Send the transaction without actually executing it
    #[clap(
        long,
        env,
        takes_value = false,
        alias = "dry",
        alias = "dryrun",
        alias = "simulate",
        alias = "sim"
    )]
    dry_run: bool,

    /// Serialize the unsigned transaction and print it to stdout
    /// Instructions which require private key signer (e.g. zero-copy account allocations) will be executed immediately
    #[clap(long, env, takes_value = false, alias = "multi", alias = "ms")]
    multisig: bool,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Actions {
    /// Download the remote oracle mapping in the provided mapping file
    #[clap(arg_required_else_help = true)]
    CreateAta {
        /// Mint
        #[clap(long, parse(try_from_str))]
        mint: Pubkey,
    },
    #[clap(arg_required_else_help = true)]
    CreateMint {
        /// Amount to mint to the admin's ata
        #[clap(short, long)]
        supply: Option<u64>,
        /// File to output the mint secret key
        #[clap(short, long, parse(from_os_str))]
        out: PathBuf,
    },
    #[clap(arg_required_else_help = true)]
    InitPool {
        /// Pool config file
        #[clap(long, parse(from_os_str))]
        config: PathBuf,
        /// Token A token account to fund the pool with, else pool admin ata
        #[clap(long, parse(try_from_str))]
        token_a_ata: Option<Pubkey>,
        /// Token B token account to fund the pool with, else pool admin ata
        #[clap(long, parse(try_from_str))]
        token_b_ata: Option<Pubkey>,
    },
    #[clap(arg_required_else_help = true)]
    UpdatePool {
        #[clap(short, long, parse(try_from_str))]
        pool: Pubkey,
        #[clap(short, long)]
        mode: UpdatePoolConfigMode,
        #[clap(short, long)]
        value: String,
    },
    #[clap(arg_required_else_help = true)]
    PrintPool {
        /// Reserve pubkey
        #[clap(short, long, parse(try_from_str))]
        pool: Pubkey,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Args = Args::parse();
    info!("Starting with args {:#?}", args);

    tracing_subscriber::fmt().compact().init();

    let payer = read_keypair_file(args.keypair).expect("Keypair file not found or invalid");
    let payer_pubkey = payer.pubkey();
    let admin = args.signer.unwrap_or_else(|| payer.pubkey());
    let commitment = CommitmentConfig::confirmed();

    let rpc_client = RpcClient::new_with_commitment(args.url.url().to_string(), commitment);
    let client = OrbitLink::new(
        rpc_client,
        Some(payer),
        None,
        commitment,
        Some(payer_pubkey),
    )
    .unwrap();
    let config = Config {
        program_id: args.program,
        dry_run: args.dry_run,
        multisig: args.multisig,
    };
    let hyperplane_client = HyperplaneClient::new(client, config).await?;

    if hyperplane_client.config.dry_run {
        info!("Dry-run mode \x1b[32mENABLED\x1b[0m");
    }
    if hyperplane_client.config.multisig {
        info!("Multisig mode \x1b[32mENABLED\x1b[0m");
    }

    match args.action {
        Actions::CreateAta { mint } => command::create_ata(&hyperplane_client, admin, mint).await,
        Actions::CreateMint { out, supply } => {
            command::create_mint(&hyperplane_client, out, admin, supply).await
        }
        Actions::InitPool {
            config,
            token_a_ata,
            token_b_ata,
        } => {
            command::initialize_pool(&hyperplane_client, admin, config, token_a_ata, token_b_ata)
                .await
        }
        Actions::UpdatePool { pool, mode, value } => {
            command::update_pool(&hyperplane_client, admin, pool, mode, value).await
        }
        Actions::PrintPool { pool } => command::print_pool(&hyperplane_client, pool).await,
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::*;

    #[test]
    pub fn test_parsing_dry_run() {
        let pool = Pubkey::new_unique();
        let signer = Pubkey::new_unique();
        let withdrawals_only_string = "true".to_string();
        let expected = Args {
            keypair: PathBuf::from("../../test/test/admin.json"),
            url: Cluster::from_str("localnet").unwrap(),
            program: hyperplane::ID,
            dry_run: true,
            multisig: false,
            signer: Some(signer),
            action: Actions::UpdatePool {
                pool,
                mode: UpdatePoolConfigMode::WithdrawalsOnly,
                value: withdrawals_only_string.clone(),
            },
        };

        let aliases = ["--dry-run", "--dryrun", "--dry", "--simulate", "--sim"];

        for alias in aliases.iter() {
            let actual = Args::parse_from([
                "",
                alias,
                "-k",
                "../../test/test/admin.json",
                "--signer",
                &signer.to_string(),
                "update-pool",
                "--pool",
                &pool.to_string(),
                "--mode",
                "WithdrawalsOnly",
                "--value",
                &withdrawals_only_string,
            ]);

            assert_eq!(actual, expected);
        }
    }

    #[test]
    pub fn test_parsing_multisig() {
        let pool = Pubkey::new_unique();
        let signer = Pubkey::new_unique();
        let withdrawals_only_string = "true".to_string();
        let expected = Args {
            keypair: PathBuf::from("../../test/test/admin.json"),
            url: Cluster::from_str("localnet").unwrap(),
            program: hyperplane::ID,
            dry_run: false,
            multisig: true,
            signer: Some(signer),
            action: Actions::UpdatePool {
                pool,
                mode: UpdatePoolConfigMode::WithdrawalsOnly,
                value: withdrawals_only_string.clone(),
            },
        };

        let aliases = ["--multisig", "--multi", "--ms"];

        for alias in aliases.iter() {
            let actual = Args::parse_from([
                "",
                alias,
                "-k",
                "../../test/test/admin.json",
                "--signer",
                &signer.to_string(),
                "update-pool",
                "--pool",
                &pool.to_string(),
                "--mode",
                "WithdrawalsOnly",
                "--value",
                &withdrawals_only_string,
            ]);

            assert_eq!(actual, expected);
        }
    }

    #[test]
    pub fn test_parsing_update_pool_short() {
        let pool = Pubkey::new_unique();
        let withdrawals_only_string = "true".to_string();
        let x = Args::parse_from([
            "",
            "-k",
            "../../test/test/admin.json",
            "update-pool",
            "-p",
            &pool.to_string(),
            "-m",
            "WithdrawalsOnly",
            "-v",
            &withdrawals_only_string,
        ]);

        assert_eq!(
            x,
            Args {
                keypair: PathBuf::from("../../test/test/admin.json"),
                url: Cluster::from_str("localnet").unwrap(),
                program: hyperplane::ID,
                dry_run: false,
                multisig: false,
                signer: None,
                action: Actions::UpdatePool {
                    pool,
                    mode: UpdatePoolConfigMode::WithdrawalsOnly,
                    value: withdrawals_only_string,
                },
            }
        );
    }
}
