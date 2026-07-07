use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    /// Funds are locked; awaiting release by the arbiter or refund after
    /// the deadline.
    Pending,
    /// Funds have been split and paid out to the payee and fee recipient.
    Released,
    /// Funds have been returned in full to the depositor.
    Refunded,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Campaign {
    pub depositor: Address,
    pub payee: Address,
    /// Address authorized to trigger [`crate::Escrow::release`]. Typically
    /// the platform or a designated approver — never the depositor or
    /// payee alone, to keep release a distinct step from funding.
    pub arbiter: Address,
    /// Stellar Asset Contract (or any SEP-41 token) used for this campaign.
    pub token: Address,
    pub amount: i128,
    /// Platform fee in basis points (1 bps = 0.01%), applied at release.
    pub fee_bps: u32,
    pub fee_recipient: Address,
    /// Ledger timestamp after which the depositor may reclaim funds if the
    /// campaign has not been released.
    pub deadline: u64,
    pub status: CampaignStatus,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    FeeDistributor,
    CampaignCount,
    Campaign(u64),
}
