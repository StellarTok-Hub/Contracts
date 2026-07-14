#![cfg(test)]

use super::{Escrow, EscrowClient};
use fee_distribution::FeeDistribution;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

/// Stand-in for a misbehaving `fee-distribution` deployment: reports a
/// split that overstates `campaign.amount` (`fee + net > amount`), the way
/// a buggy or malicious contract repointed to via `set_fee_distributor`
/// could. Used to prove `release` doesn't blindly trust the split.
#[contract]
struct MismatchedSplitDistributor;

#[contractimpl]
impl MismatchedSplitDistributor {
    pub fn compute_split(_env: Env, amount: i128, _fee_bps: u32) -> (i128, i128) {
        (amount, amount)
    }
}

/// Stand-in for a misbehaving `fee-distribution` deployment that reports a
/// negative `fee`. Used to prove `release` doesn't blindly trust the sign
/// of either half of the split.
#[contract]
struct NegativeFeeDistributor;

#[contractimpl]
impl NegativeFeeDistributor {
    pub fn compute_split(_env: Env, amount: i128, _fee_bps: u32) -> (i128, i128) {
        (-1, amount + 1)
    }
}

struct Setup<'a> {
    env: Env,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    token_admin: StellarAssetClient<'a>,
    admin: Address,
    fee_distributor_id: Address,
    depositor: Address,
    payee: Address,
    arbiter: Address,
    fee_recipient: Address,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_issuer = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_issuer);
    let token = TokenClient::new(&env, &token_contract.address());
    let token_admin = StellarAssetClient::new(&env, &token_contract.address());

    let fee_distributor_id = env.register(FeeDistribution, ());

    // The constructor runs atomically as part of registration/deployment —
    // there is no separate `initialize` call and thus no window for anyone
    // else to race in and claim the admin role.
    let escrow_id = env.register(Escrow, (admin.clone(), fee_distributor_id.clone()));
    let escrow = EscrowClient::new(&env, &escrow_id);

    Setup {
        depositor: Address::generate(&env),
        payee: Address::generate(&env),
        arbiter: Address::generate(&env),
        fee_recipient: Address::generate(&env),
        env,
        escrow,
        token,
        token_admin,
        admin,
        fee_distributor_id,
    }
}

/// Default campaign deadline: 1000 seconds from "now" in the test ledger.
fn default_deadline(env: &Env) -> u64 {
    env.ledger().timestamp() + 1_000
}

/// Creates a campaign with the setup's standard parties and returns
/// `(campaign_id, deadline)`. Covers the common case where only `amount`
/// and `fee_bps` vary between tests.
fn create_default_campaign(s: &Setup, amount: i128, fee_bps: u32) -> (u64, u64) {
    let deadline = default_deadline(&s.env);
    let id = s.escrow.create_campaign(
        &s.depositor,
        &s.payee,
        &s.arbiter,
        &s.token.address,
        &amount,
        &fee_bps,
        &s.fee_recipient,
        &deadline,
    );
    (id, deadline)
}

#[test]
fn release_pays_fee_and_net_to_the_right_parties() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &1_000_000);
    let (id, _) = create_default_campaign(&s, 1_000_000, 250); // 2.5%

    assert_eq!(s.token.balance(&s.depositor), 0);
    assert_eq!(s.token.balance(&s.escrow.address), 1_000_000);

    s.escrow.release(&id);

    assert_eq!(s.token.balance(&s.fee_recipient), 25_000);
    assert_eq!(s.token.balance(&s.payee), 975_000);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(
        s.escrow.get_campaign(&id).status,
        super::CampaignStatus::Released
    );
}

#[test]
fn refund_after_deadline_returns_full_amount_to_depositor() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &500_000);
    let (id, deadline) = create_default_campaign(&s, 500_000, 500);

    s.env.ledger().with_mut(|li| li.timestamp = deadline + 1);
    s.escrow.refund(&id);

    assert_eq!(s.token.balance(&s.depositor), 500_000);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
}

#[test]
fn refund_before_deadline_is_rejected() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &500_000);
    let (id, _) = create_default_campaign(&s, 500_000, 500);

    let result = s.escrow.try_refund(&id);
    assert!(result.is_err());
}

#[test]
fn release_twice_is_rejected() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &200_000);
    let (id, _) = create_default_campaign(&s, 200_000, 0);

    s.escrow.release(&id);
    let result = s.escrow.try_release(&id);
    assert!(result.is_err());
}

#[test]
fn pause_blocks_new_campaigns_but_never_blocks_existing_release_or_refund() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &300_000);
    let (id, deadline) = create_default_campaign(&s, 100_000, 0);

    s.escrow.set_paused(&true);

    let result = s.escrow.try_create_campaign(
        &s.depositor,
        &s.payee,
        &s.arbiter,
        &s.token.address,
        &100_000,
        &0,
        &s.fee_recipient,
        &deadline,
    );
    assert!(result.is_err());

    // A campaign funded before the pause can still be released in full.
    s.escrow.release(&id);
    assert_eq!(s.token.balance(&s.payee), 100_000);
}

#[test]
fn fee_bps_above_the_platform_cap_is_rejected() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &100_000);

    assert!(create_campaign_try(
        &s,
        100_000,
        2_001,
        default_deadline(&s.env)
    ));
}

#[test]
fn deadline_further_than_max_campaign_duration_is_rejected() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &100_000);

    let too_far = s.env.ledger().timestamp() + super::MAX_CAMPAIGN_DURATION_SECONDS + 1;
    assert!(create_campaign_try(&s, 100_000, 0, too_far));
}

fn create_campaign_try(s: &Setup, amount: i128, fee_bps: u32, deadline: u64) -> bool {
    s.escrow
        .try_create_campaign(
            &s.depositor,
            &s.payee,
            &s.arbiter,
            &s.token.address,
            &amount,
            &fee_bps,
            &s.fee_recipient,
            &deadline,
        )
        .is_err()
}

#[test]
fn mutual_cancel_requires_both_depositor_and_payee_and_refunds_in_full() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &400_000);
    let (id, _) = create_default_campaign(&s, 400_000, 0);

    s.escrow.cancel(&id);

    assert_eq!(s.token.balance(&s.depositor), 400_000);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(
        s.escrow.get_campaign(&id).status,
        super::CampaignStatus::Refunded
    );
}

#[test]
fn cancel_after_release_is_rejected() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &150_000);
    let (id, _) = create_default_campaign(&s, 150_000, 0);

    s.escrow.release(&id);
    let result = s.escrow.try_cancel(&id);
    assert!(result.is_err());
}

#[test]
fn admin_can_rotate_admin_and_fee_distributor() {
    let s = setup();
    let new_admin = Address::generate(&s.env);
    let new_fee_distributor = s.env.register(FeeDistribution, ());

    // Rotating succeeds and the new keys are immediately usable for
    // admin-gated calls (auths are mocked in this suite, so this exercises
    // the storage write and call path, not signature enforcement itself —
    // that guarantee is covered separately below).
    s.escrow.set_fee_distributor(&new_fee_distributor);
    s.escrow.set_admin(&new_admin);
    s.escrow.set_paused(&true);
    s.escrow.set_paused(&false);

    assert_ne!(new_fee_distributor, s.fee_distributor_id);
    assert_ne!(new_admin, s.admin);
}

#[test]
fn bump_campaign_ttl_succeeds_for_existing_campaign_and_fails_for_missing_one() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &50_000);
    let (id, _) = create_default_campaign(&s, 50_000, 0);

    s.escrow.bump_campaign_ttl(&id);

    let missing_result = s.escrow.try_bump_campaign_ttl(&(id + 1));
    assert!(missing_result.is_err());
}

// --- Negative authorization tests ---
//
// Every test above runs under `env.mock_all_auths()`, which makes any
// `require_auth()` call succeed regardless of which address is asking —
// it proves the happy path works, but never that the wrong caller is
// rejected. These tests switch the environment to explicit authorization
// mode (`mock_auths(&[])`, i.e. zero pre-approved invocations) immediately
// before the privileged call under test, so the only way it can succeed is
// if `require_auth()` is not actually being enforced.

#[test]
fn release_without_arbiter_auth_fails() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &100_000);
    let (id, _) = create_default_campaign(&s, 100_000, 0);

    s.env.mock_auths(&[]);
    let result = s.escrow.try_release(&id);
    assert!(result.is_err());
}

#[test]
fn refund_without_depositor_auth_fails() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &100_000);
    let (id, deadline) = create_default_campaign(&s, 100_000, 0);
    s.env.ledger().with_mut(|li| li.timestamp = deadline + 1);

    s.env.mock_auths(&[]);
    let result = s.escrow.try_refund(&id);
    assert!(result.is_err());
}

#[test]
fn cancel_without_both_parties_auth_fails() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &100_000);
    let (id, _) = create_default_campaign(&s, 100_000, 0);

    s.env.mock_auths(&[]);
    let result = s.escrow.try_cancel(&id);
    assert!(result.is_err());
}

#[test]
fn create_campaign_without_depositor_auth_fails() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &100_000);
    let deadline = default_deadline(&s.env);

    s.env.mock_auths(&[]);
    assert!(create_campaign_try(&s, 100_000, 0, deadline));
}

#[test]
fn set_paused_without_admin_auth_fails() {
    let s = setup();

    s.env.mock_auths(&[]);
    let result = s.escrow.try_set_paused(&true);
    assert!(result.is_err());
}

#[test]
fn set_admin_without_admin_auth_fails() {
    let s = setup();
    let new_admin = Address::generate(&s.env);

    s.env.mock_auths(&[]);
    let result = s.escrow.try_set_admin(&new_admin);
    assert!(result.is_err());
}

#[test]
fn set_fee_distributor_without_admin_auth_fails() {
    let s = setup();
    let new_fee_distributor = s.env.register(FeeDistribution, ());

    s.env.mock_auths(&[]);
    let result = s.escrow.try_set_fee_distributor(&new_fee_distributor);
    assert!(result.is_err());
}

// --- Fee-distributor split validation ---
//
// `fee_distributor` is admin-repointable at any time via
// `set_fee_distributor`, with no timelock. These tests prove `release`
// doesn't blindly trust whatever split a (potentially buggy or malicious)
// deployment at that address reports back — see the comment above the
// validation in `Escrow::release`.

#[test]
fn release_rejects_a_split_that_overstates_the_campaign_amount() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &1_000_000);
    let (id, _) = create_default_campaign(&s, 1_000_000, 250);

    let bad_distributor = s.env.register(MismatchedSplitDistributor, ());
    s.escrow.set_fee_distributor(&bad_distributor);

    let result = s.escrow.try_release(&id);
    assert!(result.is_err());
    assert_eq!(s.token.balance(&s.escrow.address), 1_000_000);
}

#[test]
fn release_rejects_a_negative_fee() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &1_000_000);
    let (id, _) = create_default_campaign(&s, 1_000_000, 250);

    let bad_distributor = s.env.register(NegativeFeeDistributor, ());
    s.escrow.set_fee_distributor(&bad_distributor);

    let result = s.escrow.try_release(&id);
    assert!(result.is_err());
    assert_eq!(s.token.balance(&s.escrow.address), 1_000_000);
}

// --- Instance TTL bump ---

#[test]
fn bump_instance_ttl_is_callable_by_anyone_and_keeps_the_contract_usable() {
    let s = setup();
    s.token_admin.mint(&s.depositor, &10_000);

    // No auth is required or mocked for this call — proves it's the same
    // "callable by anyone" shape as `bump_campaign_ttl`.
    s.env.mock_auths(&[]);
    s.escrow.bump_instance_ttl();

    // Instance-backed reads/writes (admin, fee distributor, campaign
    // count) still work normally afterwards.
    s.env.mock_all_auths();
    let (id, _) = create_default_campaign(&s, 10_000, 0);
    s.escrow.release(&id);
    assert_eq!(s.token.balance(&s.payee), 10_000);
}
