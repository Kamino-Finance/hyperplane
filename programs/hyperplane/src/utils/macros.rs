/// macro to deserialize a curve account depending on the curve type
#[macro_export]
macro_rules! curve {
    ($swap_curve_info: expr, $pool: expr) => {
        match $pool.curve_type() {
            $crate::curve::base::CurveType::ConstantProduct => {
                let calculator = $crate::utils::instructions::deserialize::<
                    $crate::state::ConstantProductCurve,
                >(&$swap_curve_info)
                .unwrap();
                SwapCurve {
                    calculator: std::sync::Arc::new(calculator),
                    curve_type: $pool.curve_type(),
                }
            }
            $crate::curve::base::CurveType::ConstantPrice => {
                let calculator = $crate::utils::instructions::deserialize::<
                    $crate::state::ConstantPriceCurve,
                >(&$swap_curve_info)
                .unwrap();
                SwapCurve {
                    calculator: std::sync::Arc::new(calculator),
                    curve_type: $pool.curve_type(),
                }
            }
            $crate::curve::base::CurveType::Offset => {
                let calculator = $crate::utils::instructions::deserialize::<
                    $crate::state::OffsetCurve,
                >(&$swap_curve_info)
                .unwrap();
                SwapCurve {
                    calculator: std::sync::Arc::new(calculator),
                    curve_type: $pool.curve_type(),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! require_msg {
    ($invariant:expr, $error:expr $(,)?, $message: expr) => {
        if !($invariant) {
            ::anchor_lang::prelude::msg!($message);
            return Err(anchor_lang::error!($error));
        }
    };
}

/// Print values passed to a function
#[macro_export]
macro_rules! dbg_msg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `eprintln!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `msg!`
    // will be malformed.
    () => {
        #[cfg(not(target_arch = "bpf"))]
        println!("[{}:{}]", file!(), line!())
        #[cfg(target_arch = "bpf")]
        ::anchor_lang::prelude::msg!("[{}:{}]", file!(), line!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                #[cfg(not(target_arch = "bpf"))]
                println!("[{}:{}] {} = {:#?}",
                    file!(), line!(), stringify!($val), &tmp);
                #[cfg(target_arch = "bpf")]
                ::anchor_lang::prelude::msg!("[{}:{}] {} = {:#?}",
                    file!(), line!(), stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg_msg!($val)),+,)
    };
}

/// Macro to emit an event and return it from the program
#[macro_export]
macro_rules! emitted {
    ($event: expr) => {
        ::anchor_lang::prelude::emit!($event);
        return Ok($event);
    };
}

/// Macro to convert a value to u64, with useful error message
#[macro_export]
macro_rules! to_u64 {
    ($val: expr) => {
        u64::try_from($val).map_err(|_| {
            ::anchor_lang::prelude::msg!("Unable to convert {} to u64: {}", stringify!($val), $val);
            ::anchor_lang::error!(SwapError::ConversionFailure)
        })
    };
}

/// Macro to wrap a math operation with useful error message and line number
#[macro_export]
macro_rules! try_math {
    ($val: expr) => {
        $val.map_err(|_| {
            ::anchor_lang::prelude::msg!("[{}:{}] {}", file!(), line!(), stringify!($val));
            ::anchor_lang::error!($crate::error::SwapError::CalculationFailure)
        })
    };
}

/// Macro to wrap a math operation in a result with useful error message and line number
#[macro_export]
macro_rules! optional_math {
    ($val: expr) => {
        $val.ok_or_else(|| {
            ::anchor_lang::prelude::msg!("[{}:{}] {}", file!(), line!(), stringify!($val));
            ::anchor_lang::error!($crate::error::SwapError::CalculationFailure)
        })
    };
}
