# Deployment Runbook

> Closes **#708** — deployment pipeline and testnet validation.
>
> Related: [DEPLOY.md](./DEPLOY.md) (canary rollout and health endpoints),
> [DEPLOYMENT_CONFIGURATION.md](./DEPLOYMENT_CONFIGURATION.md),
> [MAINTENANCE_WINDOWS.md](./MAINTENANCE_WINDOWS.md) (windows, pause order,
> backups, templates), [UPGRADE_AND_ROLLBACK.md](./UPGRADE_AND_ROLLBACK.md),
> [EMERGENCY_PROCEDURES.md](./EMERGENCY_PROCEDURES.md).

> **Status of the pipeline in this repository.** The scripts under
> `scripts/deploy/` and the `Dockerfile` were added by the change that closed
> #708 and **have never been executed against a real Stellar network** — the
> author had no funded account and could not run `docker build`. Treat the
> first testnet run as a rehearsal. Everything below that requires a real
> account, a funded key, or a live network is marked **[unverified]**.

---

## 1. Scope and layout

| Path | What it is |
|------|------------|
| `Dockerfile` | Multi-stage build/test image for the workspace |
| `.dockerignore` | Keeps the build context small and free of secrets |
| `scripts/deploy/lib.sh` | Shared helpers: credentials, network resolution, mainnet gate |
| `scripts/deploy/preflight.sh` | Pre-deployment validation checklist (read-only) |
| `scripts/deploy/deploy.sh` | The single deployment code path, all networks |
| `scripts/deploy/deploy_testnet.sh` | Testnet wrapper, pins `--network testnet` |
| `scripts/deploy/deploy_mainnet.sh` | Mainnet wrapper, pins `--network mainnet` + approval gate |
| `scripts/deploy/verify_deploy.sh` | Post-deploy smoke test (read-only, no key needed) |

The pre-existing `scripts/deploy.sh` and `scripts/verify_deployment.sh` at the
repository root are **unchanged** and still work. The Makefile's `deploy-testnet`
and `validate*` targets point at them. The new `scripts/deploy/` tree exists so
that the credential handling, the approval gate and the checklist live in one
reviewable place; the two trees intentionally overlap and the old one has not
been migrated.

---

## 2. Prerequisites

### Toolchain

| Requirement | Source of truth |
|-------------|-----------------|
| Rust `stable` + `wasm32-unknown-unknown` + `rustfmt` + `clippy` | `rust-toolchain.toml` |
| Edition 2021, `resolver = "2"` | `Cargo.toml` |
| Soroban SDK 21.0.0 | `Cargo.toml` `[workspace.dependencies]` |
| Soroban CLI | `cargo install --locked soroban-cli --features opt` |
| Networks (RPC URL + passphrase) | `.soroban/config.toml` |
| Contract ids after a deploy | `config/<network>_contracts.json` |

The toolchain is **not** pinned to a version in the Dockerfile. `rust:slim`
ships rustup, which reads `rust-toolchain.toml` on first use, so the repository
remains the single source of truth for the Rust version.

The Soroban CLI is **not** vendored into the image. It is a long build and its
version tracks the Stellar protocol release, not this repository. Install it on
the host that will actually submit transactions.

### A Stellar account

Deployment needs an account that can sign. This repository contains **no**
account, no key, no mnemonic and no seed phrase, and none may ever be added —
see §7. **[unverified]** All network facts below (fee payment, funding,
Friendbot behaviour for testnet) depend on a real account and were not tested.

### Local tooling used by the scripts

`bash`, `git`, `python3` (to read/write the JSON config; no `jq` dependency),
`awk`, and `soroban` for the steps that touch the network. `preflight.sh` and
`verify_deploy.sh` are read-only and safe in CI.

---

## 3. Build

### Host build (the normal path)

```bash
make build      # cargo build --target wasm32-unknown-unknown --release
make test       # cargo test
make lint       # cargo clippy --all-targets -- -D warnings
```

> **Important and easy to get wrong.** `cargo build --workspace` only builds the
> crates listed in the root `Cargo.toml` `[workspace] members`. Four
> `contracts/*` directories currently ship a Soroban entry point but are **not**
> registered as members: `campaign`, `donation`, `withdrawal` and
> `rate_limiter`. `make build` therefore does not produce WASM for them. Build
> those explicitly with `cargo build -p <name> --target wasm32-unknown-unknown
> --release`, and note that `scripts/deploy.sh` will refuse to deploy a crate
> whose WASM is missing rather than silently skipping it. Running
> `./scripts/deploy/preflight.sh` prints the member list and the non-member list
> side by side.

### Container build

```bash
# Build and test image (runs fmt, clippy and the full test suite).
docker build --target test -t stellaraid-contracts:test .

# Build/verify image used for preflight, deploy and verification.
docker build -t stellaraid-contracts .
```

Stage order, chosen so the slow network-bound step is cached independently of
ordinary source edits:

| Stage | Base | Does |
|-------|------|------|
| `toolchain` | `rust:slim-bookworm` | Materialises `rust-toolchain.toml`; asserts the wasm target is present |
| `deps` | `toolchain` | Stages **manifests only** into `/usr/src/app` and runs `cargo fetch --locked` |
| `build` | `deps` | Copies the source tree; `cargo build --release --target wasm32-unknown-unknown --workspace` and `cargo build --workspace --tests` |
| `test` | `build` | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace --no-fail-fast` |
| `deploy-assist` | `build` | Default target. Asserts no `.env` or `.soroban/accounts` entered the context. `ENTRYPOINT ["bash"]` |

The `deps` stage works because `cargo fetch` resolves the entire dependency
graph from the workspace manifests plus the committed `Cargo.lock`; it never
needs a source file. `--locked` turns a lockfile/manifest disagreement into a
build failure instead of a silent re-resolve.

`.dockerignore` excludes `.git`, `target/`, `.env*`, `.soroban/accounts`,
`*.wasm`, `*.key`/`*.pem`/`*.keystore`/`*.seed`/`mnemonic.txt`, and `docs/`
(nothing in the workspace `include!`s from `docs/`).

---

## 4. Testnet deployment

### Step 1 — preflight (read-only, no key needed)

```bash
./scripts/deploy/preflight.sh testnet
```

It checks the toolchain, the wasm target, `resolver = "2"` and a committed
`Cargo.lock`, the Soroban CLI, that the network is defined in
`.soroban/config.toml`, that no key material is tracked in git, that every
selected WASM exists, that the config file is valid JSON and not *partially*
populated, and that the credentials the deploy step will need are present in
the environment. It prints its own tally and exits non-zero on any failure. It
then prints a human sign-off checklist it cannot verify for you.

A config with **no** ids at all is reported as a warning, not a failure: that is
the normal state before a first deploy. A **partially** populated config is a
failure, because it deploys some contracts and leaves the rest pointing at
nothing.

### Step 2 — supply credentials

Read from the environment, injected by a secret manager. Never a file in this
repository, never a command-line argument.

```bash
# A local identity created out of band. The *name* is not secret.
soroban keys generate deployer --network testnet
soroban keys fund deployer --network testnet     # [unverified] testnet only

export STELLAR_DEPLOYER_IDENTITY=deployer
export STELLAR_DEPLOYER_SECRET='...'             # injected, never committed
```

`STELLAR_DEPLOYER_SECRET` is only required so the pipeline can fail closed when
no credential is present at all. The scripts pass a *reference* to
`soroban contract deploy --source` — either the identity name from
`STELLAR_DEPLOYER_IDENTITY` or the literal string `STELLAR_DEPLOYER_SECRET` for
the CLI to resolve — so the secret never appears in an argument vector.

### Step 3 — dry run

```bash
./scripts/deploy/deploy_testnet.sh --dry-run
```

Prints the resolved network, the selected contracts, the WASM paths and the
planned actions. Submits nothing. Run it first, every time.

### Step 4 — deploy

```bash
./scripts/deploy/deploy_testnet.sh                       # all member contracts
./scripts/deploy/deploy_testnet.sh --contracts "platform_config escrow"
./scripts/deploy/deploy_testnet.sh --init --skip-build
```

The script builds, deploys each selected crate, and writes the resulting public
contract ids into `config/testnet_contracts.json` (backing the file up first).
If a deploy fails part-way it aborts and tells you not to re-run blindly, since
a re-run creates duplicate deployments.

### Step 5 — initialise

Initialisation is **never** automated. The arguments differ per contract, and
initialising with the wrong admin is not repairable without a migration. The
argument sets this repository has used previously are in the root
`scripts/deploy.sh`; per-contract requirements are in
[CONTRACTS.md](./CONTRACTS.md). Example:

```bash
soroban contract invoke --id "$ESCROW_ID" --network testnet \
  --source "$STELLAR_DEPLOYER_IDENTITY" -- \
  initialize --admin "$ADMIN_ADDRESS"
```

### Step 6 — verify

```bash
./scripts/deploy/verify_deploy.sh testnet
```

Read-only. For every contract with a non-blank id it asserts that `get_version`
answers (which doubles as the reachability probe) and that the local WASM is a
well-formed contract. It also probes `platform_config.get_config` and
`escrow.is_paused`. A contract whose id is blank is reported **SKIPPED**, never
passed.

It does **not** prove a money path works. A deploy can pass every check here and
still have a broken escrow release path. A small funded donate → escrow →
release round trip on testnet is required before sign-off.

### Step 7 — soak, then record

Run against testnet for a full soak before considering mainnet. Commit
`config/testnet_contracts.json`. **Never commit `.env`, a key, or a mnemonic.**

---

## 5. Mainnet deployment

### The approval gate

`scripts/deploy/deploy_mainnet.sh` **refuses to run by default.** Three
independent conditions must all hold:

| # | Condition | Why |
|---|-----------|-----|
| 1 | `--confirm-mainnet` on the command line | The operator asked for it explicitly, by name |
| 2 | `STELLAR_ALLOW_MAINNET=1` | An explicit opt-in in the deploying environment; a CI job without it cannot deploy |
| 3 | `STELLAR_MAINNET_CONFIRM` equals the mainnet passphrase from `.soroban/config.toml`, verbatim | The interactive-style confirmation. The operator has to have read the passphrase and typed it back. There is no default, no wildcard, no `yes` shortcut |

Condition 3 is an **intent** check, not authentication — the passphrase is
public, it is committed in `.soroban/config.toml`. Authorisation comes from the
signing key, which is read from `STELLAR_DEPLOYER_SECRET` and never printed.

The gate is implemented in `lib.sh::require_mainnet_approval` and is re-checked
inside `scripts/deploy/deploy.sh` itself, so calling that script directly with
`--network mainnet` is gated too. `scripts/deploy/deploy_testnet.sh` rejects
`--network` outright, so a mistyped flag cannot reach the mainnet path from
there.

### Additional mainnet-only requirements

`deploy_mainnet.sh` additionally demands, before it delegates:

- `MAINNET_OPERATOR` and `SECOND_APPROVER`, naming two *different* humans. A
  single-operator mainnet deploy is not permitted.
- `ROLLBACK_CONTRACT_IDS`, recording the currently live contract ids.
- `CHANGELOG_REF`, an auditable reference to the deployed revision.
- A `backups/` directory (warns, does not block, if absent).

### Procedure

```bash
# 0. Inside a maintenance window, or a declared emergency.
#    docs/MAINTENANCE_WINDOWS.md §1. Two operators present.

# 1. State backup. docs/MAINTENANCE_WINDOWS.md §3.
NETWORK=mainnet CONTRACTS="escrow:$ESCROW_ID" ./scripts/backup_contract_state.sh

# 2. Preflight, including a dry run of the gate.
./scripts/deploy/preflight.sh mainnet --confirm-mainnet
./scripts/deploy/deploy_mainnet.sh --confirm-mainnet --dry-run \
    --contracts "platform_config escrow"

# 3. Confirm the gate for real.
export STELLAR_ALLOW_MAINNET=1
export STELLAR_MAINNET_CONFIRM="$(sed -n '/name = "mainnet"/,/^$/p' \
    .soroban/config.toml | sed -n 's/^network_passphrase = "\(.*\)"$/\1/p')"
export MAINNET_OPERATOR='<name>' SECOND_APPROVER='<name>'
export ROLLBACK_CONTRACT_IDS="platform_config=$OLD_PC_ID escrow=$OLD_ESCROW_ID"
export CHANGELOG_REF="$(git rev-parse HEAD)"

# 4. Deploy.
./scripts/deploy/deploy_mainnet.sh --confirm-mainnet \
    --contracts "platform_config escrow"

# 5. Initialise, verify, smoke test — as for testnet.
./scripts/deploy/verify_deploy.sh mainnet
```

The `STELLAR_MAINNET_CONFIRM` line above reads the passphrase out of the config
so it is never typed by hand into a shell history by mistake. The point of the
condition is that a human looked at the value; if you would rather type it
yourself, do that instead — the script only compares strings.

**[unverified]** No step in §5 has been run. There is no mainnet deployment
record in this repository and no contract id to cite.

---

## 6. Rollback

The full procedure is in
[UPGRADE_AND_ROLLBACK.md §Rollback](./UPGRADE_AND_ROLLBACK.md#rollback-procedure).
Summary as it applies to a deploy driven by this pipeline:

1. **Stop.** Pause the new contract before anything else
   ([EMERGENCY_PROCEDURES.md](./EMERGENCY_PROCEDURES.md)). If the new contract
   is not yet initialised there is nothing to pause and no funds are at risk.
2. **Point traffic back.** The previous contract ids are in
   `ROLLBACK_CONTRACT_IDS` and in the pre-deploy
   `config/<network>_contracts.json` backup the deploy script wrote. The deploy
   script never deletes or overwrites a previous WASM, so the old contract is
   still live and still holds its storage.
3. **Verify the old contract.** `./scripts/deploy/verify_deploy.sh <network>`
   against a config pointing at the old ids.
4. **Do not delete anything.** Never delete the failed WASM or contract id. A
   failed deployment is evidence.
5. **Record the deployed WASM hash** for the failed build so the incident can be
   reproduced.

`docs/MAINTENANCE_WINDOWS.md §4` adds the in-window variant: pause the new
deployment, redirect, unpause the previous id, send the rollback template.

### What cannot be rolled back

- **A wrong `initialize`.** If a contract is initialised with the wrong admin,
  the admin cannot be overwritten by design ([OPERATIONAL_RUNBOOK.md
  §FAQ](./OPERATIONAL_RUNBOOK.md#faq)). Recovery needs a migration entry point
  or a new contract id plus state migration.
- **A wrong fee or fee-token configuration** set via `set_fee_bps` /
  `set_token_metadata`. These are ordinary admin writes and are not reverted by
  a deploy rollback. Set them back explicitly and say so in the incident.
- **A transaction already submitted.** Only a chain-level revert undoes it.

---

## 7. Credential policy

Non-negotiable, and enforced in `lib.sh` rather than by convention:

1. **No secret, key, mnemonic, seed phrase or passphrase is committed to this
   repository**, in any file, in any example, or as a default value.
2. Secrets are read from the **environment**, or from an external secret source
   that populates the environment, at run time.
3. Secrets are **never accepted as command-line arguments** — they would appear
   in `ps`, in shell history and in CI logs. `assert_no_secret_args` rejects
   `--secret=`, `--secret-key=`, `--mnemonic=`, `--seed=` and `S=` forms.
4. Secret **values are never printed**, in whole, in part, masked, or
   length-prefixed. Diagnostics name the variable and nothing else. The presence
   test itself runs with `set -x` suppressed so a traced shell cannot leak it.
5. **A missing credential is a hard failure** before any build or transaction.
   Nothing falls back to a default identity or generates a key automatically.
6. `assert_repo_has_no_key_material` scans `git ls-files` for `.env*`,
   `mnemonic.txt`, `*.pem`, `*.key`, `*.keystore`, `*.seed` and fails closed.
   The container build runs the equivalent check, and `.dockerignore` excludes
   all of it from the build context.
7. Backups and reports may contain contract ids and view output. They are
   gitignored. **They still must not contain a secret** — dump view functions,
   not the environment.

---

## 8. Known limitations of this pipeline

Be aware of these before trusting it:

- **Nothing here has been executed.** The author could not run `docker build`
  and had no funded Stellar account. Neither the shell scripts nor the
  Dockerfile have been run, and there is no testnet deployment record.
- **The shell scripts have not even been syntax-checked** — no `bash` was
  available in the authoring environment. Expect to fix quoting or portability
  issues on first run.
- **The workspace does not currently build.** Several pre-existing files
  (`contracts/platform_config/src/types.rs`,
  `contracts/platform_config/src/storage.rs`, `contracts/escrow/src/lib.rs`)
  are missing closing braces and will not parse. That is unrelated to #708 and
  predates it, but it means `make build` and the container build will fail
  until it is fixed.
- `verify_deploy.sh` does not compare the deployed WASM hash against the local
  artefact. Doing so by hand and recording the result in the change ticket is
  a required manual step.
- `preflight.sh` warns about non-member contract crates but does not fix the
  root `Cargo.toml`; registering them is a separate deliberate change.
- `config/mainnet_contracts.json` does not exist. It is intentionally
  uncommitted; copy `config/testnet_contracts.json` and fill the ids in by hand
  after a real mainnet deploy.
- The old root-level `scripts/deploy.sh` still deploys with
  `--source "$ADMIN_SECRET"`, which puts a secret in an argument vector. It is
  unchanged because it is outside #708's scope. **Prefer `scripts/deploy/`.**
