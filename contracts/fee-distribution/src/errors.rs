use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// `fee_bps` exceeds `BPS_DENOMINATOR` (100.00%).
    InvalidFeeBps = 1,
    /// `amount` is negative.
    InvalidAmount = 2,
}
