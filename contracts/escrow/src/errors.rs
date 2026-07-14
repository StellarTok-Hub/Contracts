use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    Paused = 3,
    InvalidAmount = 4,
    InvalidFeeBps = 5,
    InvalidDeadline = 6,
    CampaignNotFound = 7,
    InvalidStatus = 8,
    NotYetExpired = 9,
    /// The configured `fee-distribution` contract returned a `(fee, net)`
    /// pair that doesn't actually account for the campaign's `amount`
    /// (negative parts, or `fee + net != amount`). Since the escrow holds
    /// every campaign's funds in one pooled balance, blindly trusting a
    /// bad split here would let a misconfigured or malicious
    /// fee-distributor drain funds belonging to other campaigns.
    InvalidSplit = 10,
}
