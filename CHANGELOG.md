# Changelog

Notable changes to the contracts in this repository. Each entry names the
affected contract and function — see git history for full diffs.

## Unreleased

- `escrow::release` now rejects a `(fee, net)` split from the configured
  `fee_distributor` unless `fee >= 0`, `net >= 0`, and `fee + net ==
  campaign.amount`, returning `Error::InvalidSplit` instead of paying it
  out. Previously the split was trusted unconditionally, so a bad or
  malicious contract repointed via `set_fee_distributor` could have
  overdrawn the shared pooled escrow balance.
- `escrow` gains a public `bump_instance_ttl()`, mirroring
  `bump_campaign_ttl()`, so the contract's own instance storage (admin,
  paused flag, fee-distributor address) can be kept alive independently
  of new-campaign activity.
- `PausedUpdated.paused` is now an event topic, matching every other
  indexed field on the module's events.
- Added `cargo-deny` (`deny.toml`) to CI for license/advisory/source
  policy checks.
