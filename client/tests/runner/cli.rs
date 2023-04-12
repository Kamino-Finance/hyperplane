use std::{process::Output, str::FromStr};

use anchor_client::solana_sdk::pubkey::Pubkey;
use hyperplane_client::client::Config;
use regex::Regex;
use tokio::process::Command;

use crate::runner::{file, file::key::ADMIN_KEY_FILE};

pub async fn create_mint(name: String, initial_supply: u64) -> Pubkey {
    let key_path = file::mint::get_mint_key_file(name);

    let status = cli_command("create-mint", Config::default())
        .arg("--out")
        .arg(&key_path)
        .arg("--supply")
        .arg(initial_supply.to_string())
        .status()
        .await
        .expect("create_mint::exception");

    if !status.success() {
        panic!("create_mint::failed");
    }

    println!("create_mint::success");

    file::mint::get_mint_key(key_path)
}

pub async fn init_pool(config_path: String, config: Config) -> Pubkey {
    let output = cli_command("init-pool", config)
        .arg("--config")
        .arg(config_path)
        .output()
        .await
        .expect("init_pool::exception");

    if output.status.code() != Some(0) {
        let output_str = get_string_from_stderr(&output);
        panic!("init_pool::failed\n\n{output_str}");
    }

    let output_str = get_string_from_stdout(&output);
    let regex = Regex::new(r"Pool: ([\w\d]+)").unwrap();
    let pool = regex
        .captures(&output_str)
        .unwrap_or_else(|| panic!("Cannot parse pool from init-pool response:\n\n{output_str}"))
        .get(1)
        .unwrap()
        .as_str();
    println!("{}", output_str);
    println!("init_pool::success");
    println!("Pool: {pool}");

    Pubkey::from_str(pool).unwrap()
}

pub async fn print_pool(pool: &Pubkey) {
    let output = cli_command("print-pool", Config::default())
        .arg("--pool")
        .arg(pool.to_string())
        .output()
        .await
        .expect("print_pool::exception");

    if output.status.code() != Some(0) {
        let output_str = get_string_from_stderr(&output);
        panic!("print_pool::failed\n\n{output_str}");
    }
    let output_str = get_string_from_stdout(&output);
    println!("print_pool::success\n\n{output_str}");
}

fn cli_command(cmd: &str, config: Config) -> Command {
    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("--bin")
        .arg("hyperplane")
        .arg("--")
        .arg("-u")
        .arg("http://127.0.0.1:8899")
        .arg("-p")
        .arg(hyperplane::id().to_string())
        .arg("-k")
        .arg(ADMIN_KEY_FILE);

    if config.dry_run {
        command.arg("--dry-run");
    }
    if config.multisig {
        command.arg("--multisig");
    }

    command.arg(cmd);
    command
}

fn get_string_from_stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn get_string_from_stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}
