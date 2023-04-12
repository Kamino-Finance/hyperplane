use std::{process::Stdio, time::Duration};

use tokio::process::{Child, Command};

use crate::runner::{
    anchor,
    file::key::{create_admin_keypair, ADMIN_KEY_FILE},
};

pub async fn start_and_deploy_program() -> Child {
    println!("Buidling hyperplane program...");
    anchor::build_program().await;
    println!("Starting test validator...");
    let solana_test_validator = pstart().await;
    println!("Airdropping funds to pool admin=...");
    new_admin().await;
    println!("Test validator started and program deployed!");
    solana_test_validator
}

pub async fn pstart() -> Child {
    let solana_test_validator = Command::new("solana-test-validator")
        .arg("--bpf-program")
        .arg(hyperplane::id().to_string())
        .arg("../target/deploy/hyperplane.so")
        .arg("--reset")
        .stdout(Stdio::piped())
        .spawn()
        .expect("solana-test-validator failed to execute");

    println!("Solana test validator started!");
    // todo - wait for start healthcheck
    std::thread::sleep(Duration::from_secs(7));

    solana_test_validator
}

pub async fn new_admin() {
    let admin_key = create_admin_keypair();
    let status = Command::new("solana")
        .arg("airdrop")
        .arg("100")
        .arg(ADMIN_KEY_FILE)
        .arg("--url")
        .arg("l")
        .status()
        .await
        .expect("Failed to airdrop solana account");

    if !status.success() {
        panic!("Failed to airdrop solana account");
    }
    println!("Funded admin account {}!", admin_key);
}

pub async fn kill(solana_test_validator: &mut Child) {
    solana_test_validator.kill().await.unwrap();
}
