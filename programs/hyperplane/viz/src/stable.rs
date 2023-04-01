use std::iter::Chain;

use hyperplane::{
    curve::{
        calculator::{CurveCalculator, SwapWithoutFeesResult, TradeDirection},
        stable::{MAX_AMP, MIN_AMP},
    },
    state,
};
use plotters::prelude::*;

pub fn plot(output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = SVGBackend::new(output_path, (640, 640)).into_drawing_area();
    root.fill(&WHITE)?;
    let mut chart = ChartBuilder::on(&root)
        .caption("Stableswap Curve", ("sans-serif", 30).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(40)
        .build_cartesian_2d(0_u128..30_000_u128, 0_u128..30_000_u128)?;

    chart.configure_mesh().draw()?;

    chart
        .draw_series(series(MIN_AMP, RED))?
        .label(&format!("A = {} (min amp)", MIN_AMP))
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));
    chart
        .draw_series(series(10, GREEN))?
        .label("A = 10")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GREEN));
    chart
        .draw_series(series(100, BLUE))?
        .label("A = 100")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE));
    chart
        .draw_series(series(1000, BLACK))?
        .label("A = 1000")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLACK));
    chart
        .draw_series(series(MAX_AMP, MAGENTA))?
        .label(&format!("A = {} (max amp)", MAX_AMP))
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], MAGENTA));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;

    root.present()?;

    Ok(())
}

#[allow(clippy::type_complexity)]
fn series<DB: DrawingBackend>(
    amp: u64,
    colour: RGBColor,
) -> Chain<LineSeries<DB, (u128, u128)>, LineSeries<DB, (u128, u128)>> {
    let curve = state::StableCurve::new(amp, 6, 6).unwrap();

    // Plot 2 series, one for buy x, one for sell x
    // Each series starts with a pool of 10k x and 10k y
    let ((mut sell_pool_x_amt, mut sell_pool_y_amt), (mut buy_pool_x_amt, mut buy_pool_y_amt)) =
        ((10_000_u128, 10_000_u128), (10_000_u128, 10_000_u128));
    // number of points to plot for each pool
    // a.k.a. number of swaps to simulate in each direction
    let plot_range = 1_000_u128;
    // amount to x or y to swap each iteration
    // stays constant
    let swap_amt = 100;

    let buy_x_points = (1..=plot_range).map(|_| {
        let SwapWithoutFeesResult {
            source_amount_swapped,
            destination_amount_swapped,
        } = curve
            .swap_without_fees(
                swap_amt,
                buy_pool_y_amt,
                buy_pool_x_amt,
                TradeDirection::BtoA,
            )
            .unwrap();

        buy_pool_x_amt -= destination_amount_swapped; // pool x shrinks
        buy_pool_y_amt += source_amount_swapped; // pool y grows
        println!("buy_pool_x_amt {}", buy_pool_x_amt);
        println!("buy_pool_y_amt  {}", buy_pool_y_amt);

        (buy_pool_x_amt, buy_pool_y_amt)
    });

    let sell_x_points = (1..=plot_range).map(|_| {
        let SwapWithoutFeesResult {
            source_amount_swapped,
            destination_amount_swapped,
        } = curve
            .swap_without_fees(
                swap_amt,
                sell_pool_x_amt,
                sell_pool_y_amt,
                TradeDirection::AtoB,
            )
            .unwrap();

        sell_pool_x_amt += source_amount_swapped; // pool x grows
        sell_pool_y_amt -= destination_amount_swapped; // pool y shrinks

        println!("sell_pool_x_amt {}", sell_pool_x_amt);
        println!("sell_pool_y_amt  {}", sell_pool_y_amt);

        (sell_pool_x_amt, sell_pool_y_amt)
    });

    LineSeries::new(buy_x_points, colour).chain(LineSeries::new(sell_x_points, colour))
}
