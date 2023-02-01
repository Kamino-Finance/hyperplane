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
            msg!($message);
            return Err(anchor_lang::error!($error));
        }
    };
}

/// Macro to emit an event and return it from the program
#[macro_export]
macro_rules! emitted {
    ($event: expr) => {
        anchor_lang::prelude::emit!($event);
        return Ok($event);
    };
}
