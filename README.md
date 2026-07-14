
# Contracts

**Soroban smart contracts powering the platform's decentralized trust layer.**

Rust-based smart contracts deployed on the Stellar network, providing trust-minimized escrow for brand campaign payouts and automated, programmatic fee distribution.

[![CI](https://github.com/StellarTok-Hub/Contracts/actions/workflows/ci.yml/badge.svg)](https://github.com/StellarTok-Hub/Contracts/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban](https://img.shields.io/badge/Soroban-Smart%20Contracts-7D00FF?style=flat)](https://soroban.stellar.org/)
[![Stellar](https://img.shields.io/badge/Stellar-Network-08B5E5?style=flat&logo=stellar&logoColor=white)](https://stellar.org/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](#license)

---

## Table of Contents

- [Overview](#overview)
- [Key Features](#key-features)
- [Architecture](#architecture)
- [Tech Stack](#tech-stack)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Installation](#installation)
  - [Build](#build)
  - [Test](#test)
  - [Deploy](#deploy)
- [Project Structure](#project-structure)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)

## Overview

This repository contains the on-chain logic that underpins the platform's trust layer for brand-influencer (or brand-creator) campaigns. It replaces manual, trust-based payment coordination with deterministic, auditable smart contracts running on [Soroban](https://soroban.stellar.org/), Stellar's smart contract platform.

At its core, the repository implements:

- **Escrow logic** — funds committed to a campaign are locked on-chain and released automatically once agreed-upon conditions are met, removing counterparty risk for both brands and campaign participants.
- **Programmatic fee distribution** — platform fees, referral splits, and other payouts are calculated and disbursed automatically as part of contract execution, eliminating manual reconciliation.

## Key Features

- 🔒 **Trust-minimized fund custody** — campaign funds are held by the contract, not by either party. The arbiter can *approve* a release but can never trap funds: the depositor can always reclaim them after the deadline, and depositor + payee can jointly cancel early by mutual consent.
- ⚡ **Automated payouts** — settlement and fee distribution execute atomically as part of contract calls, with no manual intervention.
- 🧾 **On-chain auditability** — every deposit, release, refund, cancellation, and governance change is emitted as a typed contract event.
- 🧩 **Composable fee logic** — fee calculation lives in its own contract, called cross-contract by escrow, so the fee formula can be reasoned about, tested, and (via `set_fee_distributor`) repointed independently.
- 🛡️ **Adversarially tested authorization** — every privileged function (`release`, `refund`, `cancel`, admin actions) has a test proving it rejects calls missing the required signer, not just that the happy path works.
- 🧮 **Validated fee splits** — `release` checks that the `(fee, net)` split reported by the fee-distribution contract actually sums to the campaign's `amount` before paying anyone, so a bad or malicious `fee_distributor` (repointable by admin at any time) can't overdraw the pooled escrow balance.
- 🦀 **Built in Rust** — leverages Soroban's Rust SDK for performance, safety, and a strong type system.

Contract *governance* is deliberately **not** trust-minimized: a single `admin` key can pause new campaign creation and rotate itself or the fee-distributor address instantly, with no timelock or multisig. See the module docs in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) for the reasoning — this is a documented scope boundary, not an oversight.

Storage liveness is a separate, operational responsibility: Soroban archives storage entries that go too long without a TTL extension, so a deployer should run a keeper that periodically calls `bump_campaign_ttl` for any long-duration campaign and `bump_instance_ttl` for the contract itself — both are public, fund-safe, and exist for exactly this.

## Architecture

The contract suite is organized around two responsibilities:

```
                ┌────────────────────┐
   Brand  ───▶  │   Escrow Contract   │  ───▶  Locked Funds
                └────────────────────┘
                    │     │      │
          release() │     │      │ cancel() — mutual consent, pre-deadline
                     │     │      └────────────────────────┐
                     │     │ refund() — depositor, post-deadline
                     ▼     ▼                                ▼
              ┌────────────────────┐
              │  Fee Distribution   │  ───▶  Platform Fee / Payee
              │      Contract       │
              └────────────────────┘
```

1. A brand funds a campaign via `create_campaign`, which locks the deposit in the **escrow contract** under a per-campaign arbiter and deadline.
2. **`release`** (arbiter-approved): the **fee-distribution contract** computes the platform/net split, and both parties are paid atomically.
3. **`refund`** (depositor, after the deadline) or **`cancel`** (depositor + payee, before the deadline): the full amount returns to the depositor if release never happens.
4. **Governance** (`set_paused`, `set_admin`, `set_fee_distributor`, `bump_campaign_ttl`, `bump_instance_ttl`): admin-gated contract-level operations, separate from any individual campaign's fund safety. `bump_campaign_ttl` and `bump_instance_ttl` are the two exceptions — callable by anyone, since keeping storage alive carries no fund risk.

## Tech Stack

| Component        | Technology                                      |
|-------------------|--------------------------------------------------|
| Smart Contracts    | [Soroban](https://soroban.stellar.org/) (Rust SDK) |
| Language           | Rust                                            |
| Network            | [Stellar](https://stellar.org/)                 |
| Tooling            | `soroban-cli`, Cargo, WASM                      |
| Dependency policy  | `cargo-deny` (licenses, advisories, sources — see `deny.toml`) |

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install), stable channel, with the `wasm32v1-none` target (the target Soroban's runtime supports — plain `wasm32-unknown-unknown` on Rust 1.82+ enables WASM features the Soroban host doesn't support yet). `rustup` picks this up automatically from [`rust-toolchain.toml`](rust-toolchain.toml):
  ```bash
  rustup target add wasm32v1-none
  ```
- [Stellar CLI](https://developers.stellar.org/docs/tools/cli/install-cli) (formerly `soroban-cli`):
  ```bash
  cargo install --locked stellar-cli
  ```

### Installation

```bash
git clone https://github.com/StellarTok-Hub/Contracts.git
cd Contracts
```

### Build

```bash
stellar contract build
```

Compiled WASM artifacts are output to `target/wasm32v1-none/release/`.

### Test

```bash
cargo test
```

### Deploy

Deploy `fee-distribution` first — `escrow` needs its contract address:

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/fee_distribution.wasm \
  --source <your-account> \
  --network testnet
```

`escrow` takes constructor arguments (`admin`, `fee_distributor`). Pass them
after `--` so they run atomically with deployment, in the same ledger
operation — this is what closes off any window for someone else to race in
and claim the admin role before you initialize it yourself:

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/escrow.wasm \
  --source <your-account> \
  --network testnet \
  -- \
  --admin <admin-address> \
  --fee_distributor <fee-distribution-contract-id>
```

## Project Structure

```
Contracts/
├── contracts/
│   ├── escrow/                    # Campaign fund custody: create/release/refund/cancel + governance
│   │   └── src/
│   │       ├── lib.rs             # Contract entry points
│   │       ├── types.rs           # Campaign, CampaignStatus, storage keys
│   │       ├── errors.rs          # Contract error codes
│   │       ├── events.rs          # Typed contract events
│   │       └── test.rs            # Unit + adversarial auth tests
│   └── fee-distribution/          # Stateless platform/net fee-split calculation
│       └── src/
│           ├── lib.rs
│           ├── errors.rs
│           └── test.rs
├── .github/workflows/ci.yml       # cargo-deny, fmt, clippy, test, wasm build on every push/PR
├── Cargo.toml                     # Workspace manifest
├── deny.toml                      # cargo-deny license/advisory/source policy
├── rust-toolchain.toml            # Pins the toolchain + wasm32v1-none target
├── CHANGELOG.md
├── SECURITY.md
├── CONTRIBUTING.md
└── README.md
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for local setup, the test/lint commands CI enforces, and pull request expectations.

## Security

Smart contracts manage real funds — please report suspected vulnerabilities privately rather than via public issues. See [SECURITY.md](SECURITY.md) for scope and reporting instructions.

## License

Licensed under the [Apache License 2.0](LICENSE).
