#![cfg(test)]

use super::{FeeDistribution, FeeDistributionClient};
use soroban_sdk::Env;

fn client(env: &Env) -> FeeDistributionClient<'_> {
    let id = env.register(FeeDistribution, ());
    FeeDistributionClient::new(env, &id)
}

#[test]
fn splits_with_floor_rounding_favoring_net() {
    let env = Env::default();
    let client = client(&env);

    // 2.5% of 1_000_000 = 25_000 exactly.
    let (fee, net) = client.compute_split(&1_000_000, &250);
    assert_eq!(fee, 25_000);
    assert_eq!(net, 975_000);
    assert_eq!(fee + net, 1_000_000);

    // Non-exact division: fee floors, remainder goes to net, nothing is lost.
    let (fee, net) = client.compute_split(&999, &333);
    assert_eq!(fee, 33); // floor(999 * 333 / 10_000) = floor(33.2667) = 33
    assert_eq!(net, 966);
    assert_eq!(fee + net, 999);
}

#[test]
fn zero_fee_returns_full_amount_as_net() {
    let env = Env::default();
    let client = client(&env);

    let (fee, net) = client.compute_split(&500, &0);
    assert_eq!(fee, 0);
    assert_eq!(net, 500);
}

#[test]
fn full_fee_returns_full_amount_as_fee() {
    let env = Env::default();
    let client = client(&env);

    let (fee, net) = client.compute_split(&500, &10_000);
    assert_eq!(fee, 500);
    assert_eq!(net, 0);
}

#[test]
fn rejects_fee_bps_over_100_percent() {
    let env = Env::default();
    let client = client(&env);

    let result = client.try_compute_split(&500, &10_001);
    assert!(result.is_err());
}

#[test]
fn rejects_negative_amount() {
    let env = Env::default();
    let client = client(&env);

    let result = client.try_compute_split(&-1, &100);
    assert!(result.is_err());
}

#[test]
fn rejects_amount_whose_scaled_product_overflows_i128() {
    let env = Env::default();
    let client = client(&env);

    // amount * fee_bps overflows i128 before the division by
    // BPS_DENOMINATOR gets a chance to bring it back into range —
    // `checked_mul` must catch this rather than wrapping or panicking.
    let result = client.try_compute_split(&i128::MAX, &10_000);
    assert!(result.is_err());
}
