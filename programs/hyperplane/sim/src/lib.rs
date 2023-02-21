extern crate core;

use num_bigint::BigUint;
use num_traits::ToPrimitive;

const DEFAULT_POOL_TOKENS: u128 = 0;
const DEFAULT_TARGET_PRICE: u128 = 1000000000000000000;
pub const MODEL_FEE_NUMERATOR: u128 = 1;
pub const MODEL_FEE_DENOMINATOR: u128 = 1000;

#[derive(Clone, Debug)]
pub struct StableSwapModel {
    pub amp_factor: u128,
    pub balances: Vec<u128>,
    pub n_coins: u8,
    pub fee: u128,
    pub target_prices: Vec<u128>,
    pub pool_tokens: u128,
}

impl StableSwapModel {
    pub fn new(amp_factor: u128, balances: Vec<u128>, n_coins: u8) -> StableSwapModel {
        Self {
            amp_factor,
            balances,
            n_coins,
            fee: 0,
            target_prices: vec![DEFAULT_TARGET_PRICE, DEFAULT_TARGET_PRICE],
            pool_tokens: DEFAULT_POOL_TOKENS,
        }
    }

    pub fn new_with_pool_tokens(
        amp_factor: u128,
        balances: Vec<u128>,
        n_coins: u8,
        pool_token_amount: u128,
    ) -> StableSwapModel {
        Self {
            amp_factor,
            balances,
            n_coins,
            fee: 0,
            target_prices: vec![DEFAULT_TARGET_PRICE, DEFAULT_TARGET_PRICE],
            pool_tokens: pool_token_amount,
        }
    }

    pub fn sim_xp(&self) -> Vec<u128> {
        self.balances
            .iter()
            .map(|x| BigUint::from(*x))
            .zip(self.target_prices.iter().map(|p| BigUint::from(*p)))
            .map(|(x, p)| x * p / BigUint::from(10_u128.pow(18)))
            .map(|x| x.to_u128().unwrap())
            .collect()
    }

    pub fn sim_d(&self) -> u128 {
        let mut d_prev = 0;
        let xp = self.sim_xp();
        let s = xp.iter().fold(0, |acc, x| acc + x);
        let mut d = s;
        let ann = self.amp_factor * self.n_coins as u128;

        let mut counter = 0;

        while d.abs_diff(d_prev) > 1 {
            let mut d_p = d;
            for x in xp.iter() {
                d_p = d_p * d / (self.n_coins as u128 * x);
            }
            d_prev = d;

            // D = (AnnS + D_P * n) * D / ((Ann - 1) * D + (n + 1) * D_P)

            let numerator = BigUint::from(ann * s + d_p * self.n_coins as u128) * BigUint::from(d);
            let denominator = BigUint::from(ann - 1) * BigUint::from(d)
                + BigUint::from(self.n_coins as u128 + 1) * BigUint::from(d_p);

            d = (numerator / denominator).to_u128().unwrap();

            counter += 1;
            if counter > 1000 {
                break;
            }
        }
        d
    }

    pub fn sim_dy(&self, i: u128, j: u128, dx: u128) -> u128 {
        return self.balances[j as usize] - self.sim_y(i, j, self.balances[i as usize] + dx);
    }

    pub fn sim_exchange(&mut self, i: u128, j: u128, dx: u128) -> u128 {
        let xp = self.sim_xp();
        let x = xp[i as usize] + dx;
        let y = self.sim_y(i, j, x);
        let dy = xp[j as usize] - y;
        let fee = dy * self.fee / 10_u128.pow(10);

        self.balances[i as usize] = x * 10_u128.pow(18) / self.target_prices[i as usize];
        self.balances[j as usize] = (y + fee) * 10_u128.pow(18) / self.target_prices[i as usize];

        dy - fee
    }

    pub fn sim_y(&self, i: u128, j: u128, x: u128) -> u128 {
        let d = self.sim_d();
        let mut xx = self.sim_xp();
        xx[i as usize] = x; // x is quantity of underlying asset brought to 1e18 precision

        let mut new_xx = vec![];
        for k in 0..self.n_coins {
            if k as u128 != j {
                new_xx.push(xx[k as usize]);
            }
        }
        // remove x[j] from xx
        xx = new_xx;
        let ann = self.amp_factor * self.n_coins as u128;
        let mut c = d;

        for y in xx.iter() {
            c = c * d / (y * self.n_coins as u128);
        }
        c = c * d / (self.n_coins as u128 * ann);
        let b = xx.iter().fold(0, |acc, x| acc + x) + d / ann;

        let mut y_prev = 0;
        let mut y = d;
        while y.abs_diff(y_prev) > 1 {
            y_prev = y;
            y = (y.pow(2) + c) / (2 * y + b - d);
        }
        y
    }

    pub fn sim_y_d(&mut self, i: u128, d: u128) -> u128 {
        let mut xx = self.sim_xp();
        let mut new_xx = vec![];
        for k in 0..self.n_coins {
            if k as u128 != i {
                new_xx.push(xx[k as usize]);
            }
        }
        // remove x[i] from xx
        xx = new_xx;
        let s = xx.iter().fold(0, |acc, x| acc + x);
        let ann = self.amp_factor * self.n_coins as u128;
        let mut c = d;
        for y in xx.iter() {
            c = c * d / (y * self.n_coins as u128);
        }
        c = c * d / (ann * self.n_coins as u128);
        let b = s + d / ann - d;
        let mut y_prev = 0;
        let mut y = d;
        while y.abs_diff(y_prev) > 1 {
            y_prev = y;
            y = (y.pow(2) + c) / (2 * y + b - d);
        }
        y
    }

    pub fn sim_remove_liquidity_imbalance(&mut self, amounts: Vec<u128>) -> u128 {
        let fee = self.fee * self.n_coins as u128 / (4 * (self.n_coins as u128 - 1));
        let old_balances = self.balances.clone();
        let mut new_balances = self.balances.clone();
        let d0 = self.sim_d();
        for i in 0..self.n_coins {
            new_balances[i as usize] -= amounts[i as usize];
        }
        self.balances = new_balances.clone();
        let d1 = self.sim_d();
        self.balances = old_balances.clone();
        let mut fees: Vec<u128> = new_balances.iter().map(|_| 0).collect();
        for i in 0..self.n_coins {
            let ideal_balance = d1 * old_balances[i as usize] / d0;
            let difference = ideal_balance.abs_diff(new_balances[i as usize]);
            fees[i as usize] = fee * difference / 10_u128.pow(10);
            new_balances[i as usize] -= fees[i as usize];
        }
        self.balances = new_balances.clone();
        let d2 = self.sim_d();
        self.balances = old_balances.clone();

        let token_amount = (d0 - d2) * self.pool_tokens / d0;

        token_amount
    }

    pub fn sim_calc_withdraw_one_coin(&mut self, token_amount: u128, i: u128) -> u128 {
        let xp = self.sim_xp();
        let sum_xp = xp.iter().fold(0, |acc, x| acc + x);
        let fee = if self.fee > 0 {
            self.fee - self.fee * xp[i as usize] / sum_xp + 5 * 10_u128.pow(5)
        } else {
            0
        };

        let d0 = self.sim_d();
        let d1 = d0 - token_amount * d0 / self.pool_tokens;
        let dy = xp[i as usize] - self.sim_y_d(i, d1);

        dy - dy * fee / 10_u128.pow(10)
    }
}
