#![allow(clippy::integer_arithmetic)]

/// An off-chain implementation of the stable swap invariant
///
/// Differences from smart contract impl:
///
/// - Use unlimited size numbers (BigInt), scaled to 18 dp
/// - More iterations to solve y or D (1000 vs 256)
/// - Use negative numbers when solving y
/// - Uses standard (unchecked) arithmetic - it is expected to run under test or debug mode therefore overflow checks will be enabled
///
extern crate core;

use num_bigint::BigInt;
use num_traits::{One, ToPrimitive, Zero};

pub const MAX_ITERATIONS: u64 = 1_000;
const DEFAULT_POOL_TOKENS: u128 = 0;
const DEFAULT_TARGET_PRICE: u128 = 1_000_000_000_000_000_000;
pub const MODEL_FEE_NUMERATOR: u128 = 1;
pub const MODEL_FEE_DENOMINATOR: u128 = 1000;

#[derive(Clone, Debug)]
pub struct StableSwapModel {
    pub amp_factor: BigInt,
    pub balances: Vec<BigInt>,
    pub rates: Vec<BigInt>,
    pub n_coins: BigInt,
    pub fee: BigInt,
    pub target_prices: Vec<BigInt>,
    pub pool_tokens: BigInt,
}

impl StableSwapModel {
    pub fn new(
        amp_factor: u128,
        balances: Vec<u128>,
        rates: Vec<u128>,
        n_coins: u8,
    ) -> StableSwapModel {
        Self::new_with_pool_tokens(amp_factor, balances, rates, n_coins, DEFAULT_POOL_TOKENS)
    }

    pub fn new_with_pool_tokens(
        amp_factor: u128,
        balances: Vec<u128>,
        rates: Vec<u128>,
        n_coins: u8,
        pool_token_amount: u128,
    ) -> StableSwapModel {
        Self {
            amp_factor: BigInt::from(amp_factor),
            rates: rates.iter().map(|x| BigInt::from(*x)).collect(),
            balances: balances.iter().map(|x| BigInt::from(*x)).collect(),
            n_coins: BigInt::from(n_coins),
            fee: BigInt::zero(),
            target_prices: vec![
                BigInt::from(DEFAULT_TARGET_PRICE),
                BigInt::from(DEFAULT_TARGET_PRICE),
            ],
            pool_tokens: BigInt::from(pool_token_amount),
        }
    }

    pub fn sim_xp(&self) -> Vec<BigInt> {
        self.balances
            .iter()
            .zip(self.rates.iter())
            .map(|(x, r)| x * r)
            .zip(self.target_prices.iter())
            .map(|(x, p)| x * p / BigInt::from(10).pow(18))
            .collect()
    }

    pub fn sim_d(&self) -> u128 {
        let mut d_prev = BigInt::zero();
        let xp = self.sim_xp();
        let s = xp.iter().fold(BigInt::zero(), |acc, x| acc + x);
        let mut d = s.clone();
        let ann = &self.amp_factor * &self.n_coins;

        let mut iterations = 0;
        while d.abs_diff(&d_prev) > BigInt::one() && iterations <= MAX_ITERATIONS {
            let mut d_p = d.clone();
            for x in xp.iter() {
                d_p = d_p * &d / (&self.n_coins * x);
            }
            d_prev = d.clone();

            // D = (AnnS + D_P * n) * D / ((Ann - 1) * D + (n + 1) * D_P)

            let numerator = (&ann * &s + &d_p * &self.n_coins) * &d;
            let denominator = (&ann - 1) * &d + (&self.n_coins + 1) * &d_p;

            d = numerator / denominator;
            iterations += 1;
        }
        d.to_u128().unwrap()
    }

    pub fn sim_dy(&self, i: u128, j: u128, dx: u128) -> u128 {
        self.balances[j as usize].to_u128().unwrap()
            - self.sim_y(i, j, self.balances[i as usize].to_u128().unwrap() + dx)
    }

    pub fn sim_exchange(&mut self, i: u128, j: u128, dx: u128) -> u128 {
        let xp = self.sim_xp();
        let x = &xp[i as usize] + BigInt::from(dx) * &self.rates[i as usize];
        let y = self.sim_y(i, j, x.to_u128().unwrap());
        let dy = &xp[j as usize] - y;
        let fee = &dy * &self.fee / BigInt::from(10).pow(10);

        self.balances[i as usize] = &x * BigInt::from(10).pow(18) / &self.target_prices[i as usize];
        self.balances[j as usize] =
            (y + &fee) * BigInt::from(10).pow(18) / &self.target_prices[i as usize];

        ((&dy - &fee) / &self.rates[j as usize]).to_u128().unwrap()
    }

    pub fn sim_y(&self, i: u128, j: u128, x: u128) -> u128 {
        let d = BigInt::from(self.sim_d());
        let mut xx = self.sim_xp();
        xx[i as usize] = BigInt::from(x); // x is quantity of underlying asset brought to 1e18 precision

        let mut new_xx = vec![];
        for (k, amt) in xx.iter().enumerate().take(self.n_coins.to_usize().unwrap()) {
            if k as u128 != j {
                new_xx.push(amt.clone());
            }
        }
        // remove x[j] from xx
        xx = new_xx;
        let ann = &self.amp_factor * &self.n_coins;
        let mut c = d.clone();

        // c = D**n+1 / n**n * P * Ann
        for y in xx.iter() {
            c = &c * &d / (y * &self.n_coins);
        }
        c = &c * &d / (&self.n_coins * &ann);

        // note - b is negative here, whereas in the smart contract D is subtracted from the denominator with each calculation (see below)
        // the smart contract is less efficient, but avoids negative numbers
        // b = (S + D / Ann) - D    <- simulation
        // b = (S + D / Ann)        <- smart contract
        let b = (xx.iter().fold(BigInt::zero(), |acc, x| acc + x) + &d / &ann) - &d;

        let mut y_prev = BigInt::zero();
        let mut y = d;
        let mut iterations = 0;
        while y.abs_diff(&y_prev) > BigInt::one() && iterations <= MAX_ITERATIONS {
            y_prev = y.clone();
            // note - b is negative here, whereas in the smart contract D is subtracted from the denominator with each calculation (see below)
            // the smart contract is less efficient, but avoids negative numbers
            // b = (S + D / Ann) - D    <- simulation
            // b = (S + D / Ann)        <- smart contract
            y = (y.pow(2) + &c) / (2 * &y + &b);
            iterations += 1;
        }
        y.to_u128().unwrap()
    }

    pub fn sim_y_d(&mut self, i: u128, d: u128) -> u128 {
        let d = BigInt::from(d);
        let mut xx = self.sim_xp();
        let mut new_xx = vec![];
        for (k, amt) in xx.iter().enumerate().take(self.n_coins.to_usize().unwrap()) {
            if k as u128 != i {
                new_xx.push(amt.clone());
            }
        }
        // remove x[i] from xx
        xx = new_xx;
        let s = xx.iter().fold(BigInt::zero(), |acc, x| acc + x);
        let ann = &self.amp_factor * &self.n_coins;
        let mut c = d.clone();
        // c = D**n+1 / n**n * P * Ann
        for y in xx.iter() {
            c = &c * &d / (y * &self.n_coins);
        }
        c = &c * &d / (&ann * &self.n_coins);
        // b = (S + D / Ann)
        let b = &s + &d / &ann;
        let mut y_prev = BigInt::zero();
        let mut y = d.clone();
        while y.abs_diff(&y_prev) > BigInt::one() {
            y_prev = y.clone();
            // y = y**2 + c / 2y + b - D
            y = (y.pow(2) + &c) / (2 * &y + &b - &d);
        }
        y.to_u128().unwrap()
    }

    pub fn sim_remove_liquidity_imbalance(&mut self, amounts: Vec<u128>) -> u128 {
        let fee = &self.fee * &self.n_coins / (4 * (&self.n_coins - 1));
        let old_balances = self.balances.clone();
        let mut new_balances = self.balances.clone();
        let d0 = self.sim_d();
        for i in 0..self.n_coins.to_usize().unwrap() {
            new_balances[i] -= amounts[i];
        }
        self.balances = new_balances.clone();
        let d1 = self.sim_d();
        self.balances = old_balances.clone();
        let mut fees: Vec<BigInt> = new_balances.iter().map(|_| BigInt::zero()).collect();
        for i in 0..self.n_coins.to_usize().unwrap() {
            let ideal_balance = d1 * &old_balances[i] / d0;
            let difference = ideal_balance.abs_diff(&new_balances[i]);
            fees[i] = &fee * difference / BigInt::from(10).pow(10);
            new_balances[i] -= fees[i].clone();
        }
        self.balances = new_balances.clone();
        let d2 = self.sim_d();
        self.balances = old_balances;

        let token_amount = (d0 - d2) * &self.pool_tokens / d0;

        token_amount.to_u128().unwrap()
    }

    pub fn sim_calc_withdraw_one_coin(&mut self, token_amount: u128, i: u128) -> u128 {
        let xp = self.sim_xp();
        let sum_xp = xp.iter().fold(BigInt::zero(), |acc, x| acc + x);
        let fee = if self.fee > BigInt::zero() {
            &self.fee - &self.fee * &xp[i as usize] / &sum_xp + 5 * BigInt::from(10).pow(5)
        } else {
            BigInt::zero()
        };

        let d0 = self.sim_d();
        let d1 = d0 - token_amount * d0 / &self.pool_tokens;
        let dy = &xp[i as usize] - self.sim_y_d(i, d1.to_u128().unwrap());

        (&dy - &dy * &fee / BigInt::from(10).pow(10))
            .to_u128()
            .unwrap()
    }
}

trait AbsDiff {
    fn abs_diff(&self, other: &Self) -> Self;
}

impl AbsDiff for BigInt {
    fn abs_diff(&self, other: &BigInt) -> BigInt {
        if self > other {
            self - other
        } else {
            other - self
        }
    }
}
