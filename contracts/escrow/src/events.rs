use soroban_sdk::{contractevent, Address};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCreated {
    #[topic]
    pub campaign_id: u64,
    #[topic]
    pub depositor: Address,
    pub payee: Address,
    pub token: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignReleased {
    #[topic]
    pub campaign_id: u64,
    pub fee: i128,
    pub net: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignRefunded {
    #[topic]
    pub campaign_id: u64,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCancelled {
    #[topic]
    pub campaign_id: u64,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminUpdated {
    #[topic]
    pub new_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeDistributorUpdated {
    #[topic]
    pub new_fee_distributor: Address,
}

/// `paused` is a topic, like every other indexed field on the events in
/// this module, so an off-chain indexer can filter directly for
/// pause/unpause transitions instead of decoding every `PausedUpdated`
/// event's body.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PausedUpdated {
    #[topic]
    pub paused: bool,
}
