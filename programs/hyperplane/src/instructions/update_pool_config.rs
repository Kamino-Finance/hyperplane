use anchor_lang::prelude::*;

use crate::{
    emitted, event, set_config,
    state::{SwapPool, UpdatePoolConfigMode, UpdatePoolConfigValue},
};

pub const VALUE_BYTE_ARRAY_LEN: usize = 32;

pub fn handler(
    ctx: Context<UpdatePoolConfig>,
    mode: u16,
    value: &[u8; VALUE_BYTE_ARRAY_LEN],
) -> Result<event::UpdatePoolConfig> {
    let pool = &mut ctx.accounts.pool.load_mut()?;

    let mode = UpdatePoolConfigMode::try_from(mode)
        .map_err(|_| error!(ErrorCode::InstructionDidNotDeserialize))?;

    let value = match mode {
        UpdatePoolConfigMode::WithdrawalsOnly => {
            let value = UpdatePoolConfigValue::from_bool_bytes(value)?;
            let packed_value = value.to_u64();
            set_config!(pool, withdrawals_only, packed_value);
            value
        }
    };

    emitted!(event::UpdatePoolConfig {
        mode,
        value: value.clone()
    });
}

#[derive(Accounts)]
pub struct UpdatePoolConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut,
        has_one = admin,
    )]
    pub pool: AccountLoader<'info, SwapPool>,
}

mod utils {

    #[macro_export]
    macro_rules! set_config {
        ($pool: ident, $config: ident, &$value: ident) => {{
            ::anchor_lang::prelude::msg!(
                "Setting pool config {} -> {:?}",
                stringify!($config),
                $value
            );
            $pool.$config = *$value;
        }};
        ($pool: ident, $config: ident, $value: ident) => {{
            ::anchor_lang::prelude::msg!(
                "Setting pool config {} -> {}",
                stringify!($config),
                $value
            );
            $pool.$config = $value;
        }};
    }
}
