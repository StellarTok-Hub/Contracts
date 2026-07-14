//! Escrow for campaign payouts.
//!
//! Campaign *funds* are trust-minimized: once a depositor locks a token
//! amount into a campaign, the campaign's `arbiter` can [`Escrow::release`]
//! it (asking the `fee-distribution` contract to compute the platform/net
//! split and paying both parties atomically), but can never trap the
//! funds — the depositor can always [`Escrow::refund`] the full amount
//! once the deadline passes, and the depositor and payee can jointly
//! [`Escrow::cancel`] early by mutual consent.
//!
//! Contract *governance* is not trust-minimized: a single `admin` key can
//! halt new campaigns and rotate itself or the fee-distributor address
//! instantly, with no timelock or multisig. That's a deliberate scope
//! boundary — layering governance safeguards on top, if wanted, is a
//! product decision left to the deployer, not something this contract
//! imposes.
#![no_std]

mod errors;
mod events;
mod types;

pub use errors::Error;
pub use types::{Campaign, CampaignStatus};

use events::{
    AdminUpdated, CampaignCancelled, CampaignCreated, CampaignRefunded, CampaignReleased,
    FeeDistributorUpdated, PausedUpdated,
};
use fee_distribution::FeeDistributionClient;
use soroban_sdk::{contract, contractimpl, token, Address, Env};
use types::DataKey;

/// ~5s ledger close time on Stellar mainnet/testnet.
const LEDGERS_PER_DAY: u32 = 17_280;
const INSTANCE_BUMP_AMOUNT: u32 = 30 * LEDGERS_PER_DAY;
const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - LEDGERS_PER_DAY;
const CAMPAIGN_BUMP_AMOUNT: u32 = 90 * LEDGERS_PER_DAY;
const CAMPAIGN_LIFETIME_THRESHOLD: u32 = CAMPAIGN_BUMP_AMOUNT - LEDGERS_PER_DAY;

/// Hard ceiling on `fee_bps` at campaign creation. Set well below the
/// protocol's own 10_000 (100%) limit so a misconfigured campaign can't
/// route a depositor's entire payment to the fee recipient. Tune to your
/// platform's actual fee policy.
const MAX_FEE_BPS: u32 = 2_000; // 20%

/// Hard ceiling on how far in the future `deadline` may be set, measured
/// from campaign creation. Keeps a campaign from being created with a
/// multi-decade deadline that would need indefinite `bump_campaign_ttl`
/// upkeep to survive Soroban's storage archival window. Tune to your
/// platform's actual campaign-length policy.
const MAX_CAMPAIGN_DURATION_SECONDS: u64 = 365 * 24 * 60 * 60; // 1 year

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Runs exactly once, atomically with contract deployment (Soroban
    /// calls a function named `__constructor` as part of the same
    /// `CreateContractArgsV2` host operation that creates the contract).
    /// Unlike a plain `initialize` function invoked in a follow-up
    /// transaction, there is no window between "contract exists" and
    /// "contract is initialized" for someone else to race into.
    ///
    /// `fee_distributor` is the deployed address of the `fee-distribution`
    /// contract used to compute payout splits at release time.
    pub fn __constructor(env: Env, admin: Address, fee_distributor: Address) {
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeDistributor, &fee_distributor);
        env.storage().instance().set(&DataKey::CampaignCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Emergency circuit breaker: halts *new* campaigns only. Campaigns
    /// already funded can always be released or refunded — pausing can
    /// never trap a depositor's or payee's funds.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        Self::get_admin(&env).require_auth();
        env.storage().instance().set(&DataKey::Paused, &paused);
        PausedUpdated { paused }.publish(&env);
        Ok(())
    }

    /// Rotates the admin key. Only the current admin can call this.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        Self::get_admin(&env).require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        AdminUpdated {
            new_admin: new_admin.clone(),
        }
        .publish(&env);
        Ok(())
    }

    /// Points the contract at a different `fee-distribution` deployment,
    /// e.g. to ship a fixed or upgraded fee calculation. Only the admin
    /// can call this; it takes effect on the next `release`.
    pub fn set_fee_distributor(env: Env, fee_distributor: Address) -> Result<(), Error> {
        Self::get_admin(&env).require_auth();
        env.storage()
            .instance()
            .set(&DataKey::FeeDistributor, &fee_distributor);
        FeeDistributorUpdated {
            new_fee_distributor: fee_distributor.clone(),
        }
        .publish(&env);
        Ok(())
    }

    /// Keeps a campaign's storage entry alive past Soroban's TTL/archival
    /// window. Callable by anyone — it only extends the lifetime of an
    /// existing entry and carries no fund risk — which matters for
    /// campaigns whose `deadline` is further out than the storage TTL
    /// granted at creation.
    pub fn bump_campaign_ttl(env: Env, campaign_id: u64) -> Result<(), Error> {
        let key = DataKey::Campaign(campaign_id);
        if !env.storage().persistent().has(&key) {
            return Err(Error::CampaignNotFound);
        }
        env.storage().persistent().extend_ttl(
            &key,
            CAMPAIGN_LIFETIME_THRESHOLD,
            CAMPAIGN_BUMP_AMOUNT,
        );
        Ok(())
    }

    /// Locks `amount` of `token` from `depositor` into a new campaign.
    /// Returns the new campaign's id.
    #[allow(clippy::too_many_arguments)]
    pub fn create_campaign(
        env: Env,
        depositor: Address,
        payee: Address,
        arbiter: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        fee_recipient: Address,
        deadline: u64,
    ) -> Result<u64, Error> {
        if Self::is_paused(&env) {
            return Err(Error::Paused);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(Error::InvalidFeeBps);
        }
        let now = env.ledger().timestamp();
        if deadline <= now || deadline - now > MAX_CAMPAIGN_DURATION_SECONDS {
            return Err(Error::InvalidDeadline);
        }

        depositor.require_auth();

        token::Client::new(&env, &token).transfer(
            &depositor,
            env.current_contract_address(),
            &amount,
        );

        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CampaignCount)
            .unwrap_or(0);

        let campaign = Campaign {
            depositor: depositor.clone(),
            payee: payee.clone(),
            arbiter,
            token: token.clone(),
            amount,
            fee_bps,
            fee_recipient,
            deadline,
            status: CampaignStatus::Pending,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Campaign(id), &campaign);
        env.storage().persistent().extend_ttl(
            &DataKey::Campaign(id),
            CAMPAIGN_LIFETIME_THRESHOLD,
            CAMPAIGN_BUMP_AMOUNT,
        );
        env.storage()
            .instance()
            .set(&DataKey::CampaignCount, &(id + 1));
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        CampaignCreated {
            campaign_id: id,
            depositor,
            payee,
            token,
            amount,
        }
        .publish(&env);

        Ok(id)
    }

    /// Approves payout for a pending campaign. Computes the platform/net
    /// split via the fee-distribution contract and pays the fee recipient
    /// and payee in the same transaction.
    pub fn release(env: Env, campaign_id: u64) -> Result<(), Error> {
        let mut campaign = Self::get_campaign(env.clone(), campaign_id)?;
        if campaign.status != CampaignStatus::Pending {
            return Err(Error::InvalidStatus);
        }
        campaign.arbiter.require_auth();

        let fee_distributor = Self::get_fee_distributor(&env);
        let (fee, net) = FeeDistributionClient::new(&env, &fee_distributor)
            .compute_split(&campaign.amount, &campaign.fee_bps);

        if fee < 0 {
            return Err(Error::InvalidSplit);
        }
        if net < 0 {
            return Err(Error::InvalidSplit);
        }
        if fee + net != campaign.amount {
            return Err(Error::InvalidSplit);
        }

        // Persist the state transition before making any external calls
        // (checks-effects-interactions): a campaign can only ever be
        // released once, even if `campaign.token` were a non-standard
        // contract that tried to call back into this one mid-transfer.
        campaign.status = CampaignStatus::Released;
        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &campaign);

        let token_client = token::Client::new(&env, &campaign.token);
        let this = env.current_contract_address();
        if fee > 0 {
            token_client.transfer(&this, &campaign.fee_recipient, &fee);
        }
        if net > 0 {
            token_client.transfer(&this, &campaign.payee, &net);
        }

        CampaignReleased {
            campaign_id,
            fee,
            net,
        }
        .publish(&env);

        Ok(())
    }

    /// Returns the full escrowed amount to the depositor. Only callable
    /// once the campaign's deadline has passed without a release — the
    /// arbiter cannot block a refund past the deadline.
    pub fn refund(env: Env, campaign_id: u64) -> Result<(), Error> {
        let mut campaign = Self::get_campaign(env.clone(), campaign_id)?;
        if campaign.status != CampaignStatus::Pending {
            return Err(Error::InvalidStatus);
        }
        if env.ledger().timestamp() < campaign.deadline {
            return Err(Error::NotYetExpired);
        }
        campaign.depositor.require_auth();

        campaign.status = CampaignStatus::Refunded;
        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &campaign);

        token::Client::new(&env, &campaign.token).transfer(
            &env.current_contract_address(),
            &campaign.depositor,
            &campaign.amount,
        );

        CampaignRefunded {
            campaign_id,
            amount: campaign.amount,
        }
        .publish(&env);

        Ok(())
    }

    /// Cancels a pending campaign before its deadline and returns the full
    /// amount to the depositor. Requires authorization from *both* the
    /// depositor and the payee — mutual consent is the only way out of a
    /// campaign before the deadline or an arbiter release.
    pub fn cancel(env: Env, campaign_id: u64) -> Result<(), Error> {
        let mut campaign = Self::get_campaign(env.clone(), campaign_id)?;
        if campaign.status != CampaignStatus::Pending {
            return Err(Error::InvalidStatus);
        }
        campaign.depositor.require_auth();
        campaign.payee.require_auth();

        campaign.status = CampaignStatus::Refunded;
        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &campaign);

        token::Client::new(&env, &campaign.token).transfer(
            &env.current_contract_address(),
            &campaign.depositor,
            &campaign.amount,
        );

        CampaignCancelled {
            campaign_id,
            amount: campaign.amount,
        }
        .publish(&env);

        Ok(())
    }

    pub fn get_campaign(env: Env, campaign_id: u64) -> Result<Campaign, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .ok_or(Error::CampaignNotFound)
    }

    /// A successfully deployed `Escrow` instance always has an admin set:
    /// `__constructor` runs atomically with contract creation, and the
    /// host aborts contract creation entirely if it panics — so there is
    /// no reachable state where this contract exists but this is unset.
    fn get_admin(env: &Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    /// See [`Self::get_admin`]: set unconditionally by the constructor.
    fn get_fee_distributor(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::FeeDistributor)
            .unwrap()
    }

    fn is_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }
}

mod test;
