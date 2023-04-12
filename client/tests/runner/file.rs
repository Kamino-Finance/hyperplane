use std::path::Path;

use anchor_client::anchor_lang::prelude::*;

pub mod key {
    use anchor_client::solana_sdk::signature::{write_keypair_file, Keypair, Signer};

    use super::*;

    pub const ADMIN_KEY_FILE: &str = "test-ledger/hyperplane-cli-test-admin.json";

    pub fn create_admin_keypair() -> Pubkey {
        let admin = Keypair::new();
        write_keypair_file(&admin, ADMIN_KEY_FILE).unwrap();
        admin.pubkey()
    }
}

pub mod mint {
    use anchor_client::solana_sdk::signature::{read_keypair_file, Signer};

    use super::*;

    pub fn get_mint_key_file(name: String) -> String {
        let path = format!("test-ledger/hyperplane-cli-test-mint-{}.json", name);
        let path = Path::new(&path);
        path.to_str().unwrap().to_string()
    }

    pub fn get_mint_key(key_path: String) -> Pubkey {
        read_keypair_file(key_path).unwrap().pubkey()
    }
}

pub mod pool {
    use super::*;

    pub fn generate_config_file(token_a_mint: &Pubkey, token_b_mint: &Pubkey) -> String {
        let config_str = get_config_str(token_a_mint, token_b_mint);
        let config_path = get_config_file();
        std::fs::write(config_path.clone(), config_str).unwrap();
        config_path
    }

    fn get_config_file() -> String {
        let path = Path::new("test-ledger/hyperplane-cli-test-pool-config.json");
        path.to_str().unwrap().to_string()
    }

    fn get_config_str(token_a_mint: &Pubkey, token_b_mint: &Pubkey) -> String {
        r#"
    {
        "token_a_mint": "<TOKEN_A_MINT_PUBKEY>",
        "token_b_mint": "<TOKEN_B_MINT_PUBKEY>",
        "curve": {
            "Stable": {
                "amp": 100
            }
        },
        "fees": {
            "trade_fee_numerator": 25,
            "trade_fee_denominator": 10000,
            "owner_trade_fee_numerator": 5,
            "owner_trade_fee_denominator": 10000,
            "owner_withdraw_fee_numerator": 0,
            "owner_withdraw_fee_denominator": 10000,
            "host_fee_numerator": 5,
            "host_fee_denominator": 10000
        },
        "initial_supply": {
            "initial_supply_a": 1000000000000,
            "initial_supply_b": 1000000000000
        }
    }
    "#
        .replace("<TOKEN_A_MINT_PUBKEY>", &token_a_mint.to_string())
        .replace("<TOKEN_B_MINT_PUBKEY>", &token_b_mint.to_string())
    }
}
