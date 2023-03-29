#![allow(clippy::integer_arithmetic)]

mod stable;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    stable::plot("stable.svg")?;

    Ok(())
}
