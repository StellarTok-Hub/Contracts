# Security Policy

These contracts hold and move real funds once deployed. If you believe
you've found a vulnerability — a way to release, refund, or redirect
escrowed funds outside the paths documented in [`contracts/escrow`](contracts/escrow),
or a way to bypass the fee calculation in [`contracts/fee-distribution`](contracts/fee-distribution) —
please report it privately rather than opening a public issue or PR.

## Reporting

Email the maintainers listed in the repository's GitHub organization, or
use GitHub's private vulnerability reporting for this repository
(**Security** tab → **Report a vulnerability**) if enabled. Please include:

- The affected contract and function.
- Preconditions and a minimal reproduction (test case or transaction
  sequence) demonstrating the issue.
- Your assessment of impact (which funds/parties are affected).

## Scope

In scope: logic in `contracts/escrow` and `contracts/fee-distribution`,
including authorization checks, state transitions, and arithmetic.

Out of scope: the Soroban runtime and Stellar network itself (report those
to [Stellar's own security process](https://stellar.org/security)), and
known, documented design tradeoffs called out in this repository's README
and code comments (e.g. single-admin governance, arbiter-gated release,
the admin's ability to repoint `fee_distributor` via `set_fee_distributor`
with no timelock). Note that `escrow::release` independently validates
the `(fee, net)` split it gets back from whatever contract
`fee_distributor` currently points to — a way to make that validation
itself pass on a split that doesn't actually account for the campaign's
`amount` would be in scope.
