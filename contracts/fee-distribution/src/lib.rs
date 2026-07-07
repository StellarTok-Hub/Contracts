//! Stateless fee-splitting logic shared by contracts that need to divide a
//! token amount between a payee and a platform fee recipient.
//!
//! This contract holds no funds and no state — it is a pure calculation
//! module, deployed once and called cross-contract (e.g. by `escrow`) so the
//! fee formula lives in exactly one place and can be audited, tested, and
//! upgraded independently of the contracts that move tokens.
#![no_std]

mod errors;

pub use errors::Error;

use soroban_sdk::{contract, contractimpl, Env};

/// Denominator for basis-point fee calculations (10_000 bps = 100.00%).
pub const BPS_DENOMINATOR: i128 = 10_000;

#[contract]
pub struct FeeDistribution;

#[contractimpl]
impl FeeDistribution {
    /// Splits `amount` into `(fee, net)` given a fee rate in basis points
    /// (1 bps = 0.01%; e.g. `250` = 2.5%).
    ///
    /// The fee is floored (integer division), so any rounding remainder is
    /// left in `net` — rounding always favors the payee, never the platform.
    pub fn compute_split(_env: Env, amount: i128, fee_bps: u32) -> Result<(i128, i128), Error> {
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        if i128::from(fee_bps) > BPS_DENOMINATOR {
            return Err(Error::InvalidFeeBps);
        }

        let scaled = amount
            .checked_mul(i128::from(fee_bps))
            .ok_or(Error::InvalidAmount)?;
        let fee = scaled / BPS_DENOMINATOR;
        let net = amount - fee;

        Ok((fee, net))
    }
}

mod test;
