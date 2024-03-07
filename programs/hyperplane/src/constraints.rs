//! Various constraints as required for production environments

#[cfg(feature = "production")]
use std::env;

use anchor_lang::{
    err,
    prelude::{AccountInfo, Pubkey},
    Result,
};
use anchor_spl::token_2022::spl_token_2022::extension::{
    BaseStateWithExtensions, ExtensionType, StateWithExtensions,
};

use crate::{
    curve::{
        base::{CurveType, SwapCurve},
        fees::Fees,
    },
    error::SwapError,
};

/// Encodes fee constraints, used in multihost environments where the program
/// may be used by multiple frontends, to ensure that proper fees are being
/// assessed.
/// Since this struct needs to be created at compile-time, we only have access
/// to const functions and constructors. Since SwapCurve contains a Arc, it
/// cannot be used, so we have to split the curves based on their types.
pub struct SwapConstraints<'a> {
    /// Owner of the program
    pub owner_key: &'a str,
    /// Valid curve types
    pub valid_curve_types: &'a [CurveType],
    /// Valid fees
    pub fees: &'a Fees,
    /// token_2022 trading token blocked extensions
    pub blocked_trading_token_extensions: &'a [ExtensionType],
}

impl<'a> SwapConstraints<'a> {
    /// Checks that the provided admin is valid for the given constraints
    pub fn validate_admin(&self, admin: &Pubkey) -> Result<()> {
        let owner_key = self
            .owner_key
            .parse::<Pubkey>()
            .map_err(|_| SwapError::InvaliPoolAdmin)?;
        if &owner_key == admin {
            Ok(())
        } else {
            err!(SwapError::InvaliPoolAdmin)
        }
    }

    /// Checks that the provided curve is valid for the given constraints
    pub fn validate_curve(&self, swap_curve: &SwapCurve) -> Result<()> {
        if self
            .valid_curve_types
            .iter()
            .any(|x| *x == swap_curve.curve_type)
        {
            Ok(())
        } else {
            err!(SwapError::UnsupportedCurveType)
        }
    }

    /// Checks that the provided curve is valid for the given constraints
    pub fn validate_fees(&self, fees: &Fees) -> Result<()> {
        if fees.trade_fee_numerator >= self.fees.trade_fee_numerator
            && fees.trade_fee_denominator == self.fees.trade_fee_denominator
            && fees.owner_trade_fee_numerator >= self.fees.owner_trade_fee_numerator
            && fees.owner_trade_fee_denominator == self.fees.owner_trade_fee_denominator
            && fees.owner_withdraw_fee_numerator >= self.fees.owner_withdraw_fee_numerator
            && fees.owner_withdraw_fee_denominator == self.fees.owner_withdraw_fee_denominator
            && fees.host_fee_numerator == self.fees.host_fee_numerator
            && fees.host_fee_denominator == self.fees.host_fee_denominator
        {
            Ok(())
        } else {
            err!(SwapError::InvalidFee)
        }
    }

    /// Checks that the provided admin is valid for the given constraints
    pub fn validate_token_2022_trading_token_extensions(
        &self,
        mint_acc_info: &AccountInfo,
    ) -> Result<()> {
        let mint_data = mint_acc_info.data.borrow();
        let mint =
            StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(
                &mint_data,
            )?;
        for mint_ext in mint.get_extension_types()? {
            if self.blocked_trading_token_extensions.contains(&mint_ext) {
                return err!(SwapError::InvalidTokenExtension);
            }
        }
        Ok(())
    }
}

#[cfg(feature = "production")]
const OWNER_KEY: &str = env!("SWAP_PROGRAM_OWNER_FEE_ADDRESS");
#[cfg(feature = "production")]
const FEES: &Fees = &Fees {
    trade_fee_numerator: 0,
    trade_fee_denominator: 10000,
    owner_trade_fee_numerator: 5,
    owner_trade_fee_denominator: 10000,
    owner_withdraw_fee_numerator: 0,
    owner_withdraw_fee_denominator: 0,
    host_fee_numerator: 20,
    host_fee_denominator: 100,
};
#[cfg(feature = "production")]
const VALID_CURVE_TYPES: &[CurveType] = &[CurveType::ConstantPrice, CurveType::ConstantProduct];
#[cfg(feature = "production")]
const INVALID_TOKEN_2022_EXTENSIONS: &[ExtensionType] = &[ExtensionType::TransferFeeConfig];

/// Fee structure defined by program creator in order to enforce certain
/// fees when others use the program.  Adds checks on pool creation and
/// swapping to ensure the correct fees and account owners are passed.
/// Fees provided during production build currently are considered min
/// fees that creator of the pool can specify. Host fee is a fixed
/// percentage that host receives as a portion of owner fees
pub const SWAP_CONSTRAINTS: Option<SwapConstraints> = {
    #[cfg(feature = "production")]
    {
        Some(SwapConstraints {
            owner_key: OWNER_KEY,
            valid_curve_types: VALID_CURVE_TYPES,
            fees: FEES,
            blocked_trading_token_extensions: INVALID_TOKEN_2022_EXTENSIONS,
        })
    }
    #[cfg(not(feature = "production"))]
    {
        None
    }
};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anchor_lang::{
        prelude::{Clock, SolanaSysvar},
        solana_program::{clock::Epoch, program_option::COption},
    };
    use anchor_spl::token_2022::{
        spl_token_2022,
        spl_token_2022::extension::{
            transfer_fee::{TransferFee, TransferFeeConfig},
            StateWithExtensionsMut,
        },
    };
    use spl_pod::optional_keys::OptionalNonZeroPubkey;

    use super::*;
    use crate::{
        curve::base::CurveType, instructions::test::runner::syscall_stubs::test_syscall_stubs,
        state::ConstantProductCurve,
    };

    #[test]
    fn test_validate_fees() {
        let trade_fee_numerator = 1;
        let trade_fee_denominator = 4;
        let owner_trade_fee_numerator = 2;
        let owner_trade_fee_denominator = 5;
        let owner_withdraw_fee_numerator = 4;
        let owner_withdraw_fee_denominator = 10;
        let host_fee_numerator = 10;
        let host_fee_denominator = 100;
        let owner_key = "";
        let curve_type = CurveType::ConstantProduct;
        let valid_fees = Fees {
            trade_fee_numerator,
            trade_fee_denominator,
            owner_trade_fee_numerator,
            owner_trade_fee_denominator,
            owner_withdraw_fee_numerator,
            owner_withdraw_fee_denominator,
            host_fee_numerator,
            host_fee_denominator,
        };
        let calculator = ConstantProductCurve {
            ..Default::default()
        };
        let swap_curve = SwapCurve {
            curve_type,
            calculator: Arc::new(calculator.clone()),
        };
        let constraints = SwapConstraints {
            owner_key,
            valid_curve_types: &[curve_type],
            fees: &valid_fees,
            blocked_trading_token_extensions: &[],
        };

        constraints.validate_curve(&swap_curve).unwrap();
        constraints.validate_fees(&valid_fees).unwrap();

        let mut fees = valid_fees;
        fees.trade_fee_numerator = trade_fee_numerator - 1;
        assert_eq!(
            Err(SwapError::InvalidFee.into()),
            constraints.validate_fees(&fees),
        );
        fees.trade_fee_numerator = trade_fee_numerator;

        // passing higher fee is ok
        fees.trade_fee_numerator = trade_fee_numerator - 1;
        assert_eq!(constraints.validate_fees(&valid_fees), Ok(()));
        fees.trade_fee_numerator = trade_fee_numerator;

        fees.trade_fee_denominator = trade_fee_denominator - 1;
        assert_eq!(
            Err(SwapError::InvalidFee.into()),
            constraints.validate_fees(&fees),
        );
        fees.trade_fee_denominator = trade_fee_denominator;

        fees.trade_fee_denominator = trade_fee_denominator + 1;
        assert_eq!(
            Err(SwapError::InvalidFee.into()),
            constraints.validate_fees(&fees),
        );
        fees.trade_fee_denominator = trade_fee_denominator;

        fees.owner_trade_fee_numerator = owner_trade_fee_numerator - 1;
        assert_eq!(
            Err(SwapError::InvalidFee.into()),
            constraints.validate_fees(&fees),
        );
        fees.owner_trade_fee_numerator = owner_trade_fee_numerator;

        // passing higher fee is ok
        fees.owner_trade_fee_numerator = owner_trade_fee_numerator - 1;
        assert_eq!(constraints.validate_fees(&valid_fees), Ok(()));
        fees.owner_trade_fee_numerator = owner_trade_fee_numerator;

        fees.owner_trade_fee_denominator = owner_trade_fee_denominator - 1;
        assert_eq!(
            Err(SwapError::InvalidFee.into()),
            constraints.validate_fees(&fees),
        );
        fees.owner_trade_fee_denominator = owner_trade_fee_denominator;

        let swap_curve = SwapCurve {
            curve_type: CurveType::ConstantPrice,
            calculator: Arc::new(calculator),
        };
        assert_eq!(
            Err(SwapError::UnsupportedCurveType.into()),
            constraints.validate_curve(&swap_curve),
        );
    }

    #[test]
    fn test_validate_admin() {
        let key = Pubkey::new_unique();
        let owner_key = &key.to_string();
        let fees = Fees::default();
        let constraints = SwapConstraints {
            owner_key,
            valid_curve_types: &[],
            fees: &fees,
            blocked_trading_token_extensions: &[],
        };

        constraints.validate_admin(&key).unwrap();
    }

    #[test]
    fn test_validate_admin_fail_when_invalid_admin() {
        let key = Pubkey::new_unique();
        let owner_key = &key.to_string();
        let fees = Fees::default();
        let constraints = SwapConstraints {
            owner_key,
            valid_curve_types: &[],
            fees: &fees,
            blocked_trading_token_extensions: &[],
        };

        let res = constraints.validate_admin(&Pubkey::new_unique());
        assert_eq!(res.err(), Some(SwapError::InvaliPoolAdmin.into()));
    }

    #[test]
    fn test_validate_trading_token_extensions_when_all_allowed() {
        test_syscall_stubs();

        let mut mint_data = mint_with_fee_data();
        mint_with_transfer_fee(&mut mint_data, 10);

        let key = Pubkey::new_unique();
        let mut lamports = u64::MAX;
        let token_program = spl_token_2022::id();
        let mint_info = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports,
            &mut mint_data,
            &token_program,
            false,
            Epoch::default(),
        );

        let owner_key = "";
        let fees = Fees::default();
        let constraints = SwapConstraints {
            owner_key,
            valid_curve_types: &[],
            fees: &fees,
            blocked_trading_token_extensions: &[],
        };

        constraints
            .validate_token_2022_trading_token_extensions(&mint_info)
            .unwrap();
    }

    #[test]
    fn test_validate_trading_token_extensions_fail_when_transfer_fee_blocked() {
        test_syscall_stubs();

        let mut mint_data = mint_with_fee_data();
        mint_with_transfer_fee(&mut mint_data, 10);

        let key = Pubkey::new_unique();
        let mut lamports = u64::MAX;
        let token_program = spl_token_2022::id();
        let mint_info = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports,
            &mut mint_data,
            &token_program,
            false,
            Epoch::default(),
        );

        let owner_key = "";
        let fees = Fees::default();
        let constraints = SwapConstraints {
            owner_key,
            valid_curve_types: &[],
            fees: &fees,
            blocked_trading_token_extensions: &[ExtensionType::TransferFeeConfig],
        };

        let res = constraints.validate_token_2022_trading_token_extensions(&mint_info);
        assert_eq!(res.err(), Some(SwapError::InvalidTokenExtension.into()));
    }

    fn mint_with_transfer_fee(mint_data: &mut [u8], transfer_fee_bps: u16) {
        let mut mint =
            StateWithExtensionsMut::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack_uninitialized(mint_data)
                .unwrap();
        let extension = mint.init_extension::<TransferFeeConfig>(true).unwrap();
        extension.transfer_fee_config_authority = OptionalNonZeroPubkey::default();
        extension.withdraw_withheld_authority = OptionalNonZeroPubkey::default();
        extension.withheld_amount = 0u64.into();

        let epoch = Clock::get().unwrap().epoch;
        let transfer_fee = TransferFee {
            epoch: epoch.into(),
            transfer_fee_basis_points: transfer_fee_bps.into(),
            maximum_fee: u64::MAX.into(),
        };
        extension.older_transfer_fee = transfer_fee;
        extension.newer_transfer_fee = transfer_fee;

        mint.base.decimals = 6;
        mint.base.is_initialized = true;
        mint.base.mint_authority = COption::Some(Pubkey::new_unique());
        mint.pack_base();
        mint.init_account_type().unwrap();
    }

    fn mint_with_fee_data() -> Vec<u8> {
        vec![
            0;
            ExtensionType::try_calculate_account_len::<
                anchor_spl::token_2022::spl_token_2022::state::Mint,
            >(&[ExtensionType::TransferFeeConfig])
            .unwrap()
        ]
    }
}
