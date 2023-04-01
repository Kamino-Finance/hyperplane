mod runner;

use crate::runner::{cli, file, validator};
use hyperplane_client::client::Config;

#[tokio::test]
pub async fn init_pool() {
    let mut solana_test_validator = validator::start_and_deploy_program().await;

    let token_a_mint = cli::create_mint("a".to_string(), 1000000000000).await;
    let token_b_mint = cli::create_mint("b".to_string(), 1000000000000).await;
    let config_path = file::pool::generate_config_file(&token_a_mint, &token_b_mint);
    let pool = cli::init_pool(config_path, Config::default()).await;

    cli::print_pool(&pool).await;

    validator::kill(&mut solana_test_validator).await;
}
