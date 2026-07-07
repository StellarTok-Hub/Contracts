# Contributing

## Setup

Install Rust (stable) with the `wasm32v1-none` target — `rustup` picks
this up automatically from [`rust-toolchain.toml`](rust-toolchain.toml)
when you run any `cargo`/`rustup` command in this repo:

```bash
rustup target add wasm32v1-none
```

## Before opening a pull request

Run the same checks CI runs, in this order — each one catches a
different class of problem, and CI will fail on any of them:

```bash
cargo fmt --all -- --check      # formatting
cargo clippy --workspace --all-targets -- -D warnings   # lints, zero warnings allowed
cargo test --workspace          # unit + adversarial auth tests
cargo build --target wasm32v1-none --release --workspace # confirms it actually deploys
```

## Writing tests for privileged functions

Every function that calls `some_address.require_auth()` needs a test
proving it *rejects* a call missing that authorization, not just that the
happy path works — `env.mock_all_auths()` (used for setup and happy-path
tests) makes every `require_auth()` succeed regardless of caller, so it
proves nothing about access control on its own. Use
`env.mock_auths(&[])` immediately before the call under test to prove the
call fails without it — see `release_without_arbiter_auth_fails` and
similar tests in `contracts/escrow/src/test.rs` for the pattern.

## Dependency updates

This project pins `Cargo.lock`. If a `cargo update` changes the
resolved graph, run the full check list above before committing the
lockfile change — a prior incident in this repo involved an upstream
crate (`soroban-env-host`) declaring an unbounded `ed25519-dalek`
version requirement that resolved to an incompatible major version and
broke the build with no code change on our side. Dependabot is
configured to open update PRs individually so CI catches this per-bump
rather than in a batch.

## Scope decisions you don't need to re-litigate

A few design choices are intentional and documented at the point they
matter — read the module doc comment in `contracts/escrow/src/lib.rs`
and [SECURITY.md](SECURITY.md) before proposing changes to:

- Single-key `admin` governance (no timelock/multisig).
- Per-campaign `arbiter` as the sole release trigger.
- Sequential campaign IDs (a throughput tradeoff, not a bug).

If you think one of these should change, open an issue describing the
tradeoff rather than a PR — these are product decisions, not oversights.
