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
}
