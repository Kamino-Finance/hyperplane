use hyperplane::state::{UpdatePoolConfigMode, UpdatePoolConfigValue};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoolConfigValue {
    WithdrawalsOnly(bool),
}

impl PoolConfigValue {
    pub fn new(mode: UpdatePoolConfigMode, value: UpdatePoolConfigValue) -> Self {
        #[allow(unreachable_patterns)] // remove when more modes + values are added
        match (mode, value) {
            (UpdatePoolConfigMode::WithdrawalsOnly, UpdatePoolConfigValue::Bool(val)) => {
                PoolConfigValue::WithdrawalsOnly(val)
            }
            (
                // explicitly match all other cases to catch new modes at compile time
                UpdatePoolConfigMode::WithdrawalsOnly,
                _,
            ) => {
                panic!("Invalid value for update lending market mode: {mode:?}");
            }
        }
    }

    pub fn new_from_str(mode: UpdatePoolConfigMode, value: String) -> PoolConfigValue {
        let parsed_value = match (mode, value) {
            (UpdatePoolConfigMode::WithdrawalsOnly, val) => {
                UpdatePoolConfigValue::Bool(val.parse::<bool>().unwrap())
            }
        };
        PoolConfigValue::new(mode, parsed_value)
    }
}

impl From<PoolConfigValue> for hyperplane::instruction::UpdatePoolConfig {
    fn from(value: PoolConfigValue) -> Self {
        match value {
            PoolConfigValue::WithdrawalsOnly(val) => hyperplane::instruction::UpdatePoolConfig {
                mode: UpdatePoolConfigMode::WithdrawalsOnly as u16,
                value: UpdatePoolConfigValue::Bool(val).to_bytes(),
            },
        }
    }
}

impl From<PoolConfigValue> for hyperplane::ix::UpdatePoolConfig {
    fn from(value: PoolConfigValue) -> Self {
        match value {
            PoolConfigValue::WithdrawalsOnly(val) => hyperplane::ix::UpdatePoolConfig::new(
                UpdatePoolConfigMode::WithdrawalsOnly,
                UpdatePoolConfigValue::Bool(val),
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use anchor_client::anchor_lang::prelude::Pubkey;

    use super::*;

    #[test]
    pub fn test_new_market_config_bool() {
        let config_val = PoolConfigValue::new_from_str(
            UpdatePoolConfigMode::WithdrawalsOnly,
            "true".to_string(),
        );
        assert_eq!(config_val, PoolConfigValue::WithdrawalsOnly(true));
    }

    #[test]
    #[should_panic]
    pub fn test_new_market_config_unparseable_bool() {
        PoolConfigValue::new_from_str(
            UpdatePoolConfigMode::WithdrawalsOnly,
            Pubkey::new_unique().to_string(), // pubkey string instead of bool
        );
    }
}
