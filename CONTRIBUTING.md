# Contributing to Lumora Contracts

## Dev Environment Setup

1. Install Rust: https://rustup.rs
2. Add Wasm target:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```
3. Install Soroban CLI:
   ```bash
   cargo install --locked soroban-cli --features opt
   ```
4. Copy env file:
   ```bash
   cp .env.example .env
   ```

## Running Tests

```bash
cargo test
```

## Pre-commit Hooks

The hooks in `.pre-commit-config.yaml` run the formatting, linting and
file-hygiene checks at commit time, so a problem is caught while you still
have the context loaded rather than in a review a day later.

### Install

```bash
pipx install pre-commit      # or: brew install pre-commit
pre-commit install
```

`pre-commit install` writes `.git/hooks/pre-commit`. It is a per-clone
setting, so everyone who clones needs to run it once.

### Verify

```bash
pre-commit run --all-files && echo "hooks are live"
```

Run that once after installing. It is the only way to be sure the hooks
actually work on your machine rather than assuming they do.

### What runs, and when

| Hook | Stage | What it does |
|---|---|---|
| `trailing-whitespace`, `end-of-file-fixer`, `mixed-line-ending` | pre-commit | file hygiene (auto-fixes and re-stages) |
| `check-merge-conflict` | pre-commit | fails if a conflict marker was committed |
| `check-yaml` | pre-commit | catches a broken workflow before it lands |
| `check-toml`, `check-json` | pre-commit | manifest and config syntax |
| `check-added-large-files` | pre-commit | blocks a committed `target/` or `.wasm` |
| `check-executables-have-shebangs` | pre-commit | a shell script that lost its shebang |
| `detect-private-key` | pre-commit | a PEM or OpenSSH key about to be committed |
| `fmt` | pre-commit | `rustfmt` on the staged `.rs` files |
| `clippy` | pre-commit | `cargo clippy` on the crates you touched |
| `ts-lint` | pre-commit | `eslint` over `sdk/bindings` |

### Bypassing a hook

Every hook id is a `SKIP` keyword:

```bash
SKIP=clippy git commit -m "wip: bisect a flaky test"
```

`git commit` prints what you skipped, and CI re-runs the same gates, so a
bypassed hook fails the PR rather than shipping. The hooks are a fast local
loop — never the only line of defence.

### Design notes

**Why clippy is scoped to the crates you touched.** A full-workspace clippy on
this repository takes minutes. `scripts/precommit_clippy.sh` maps the staged
`.rs` files to their owning crates and runs `--all-targets -- -D warnings` on
just those, so the lint set is identical but the wall-clock is not. It always
adds `shared`, because a change there can break every contract that depends on
it. CI runs the full workspace pass; that is the real gate.

**Why the rust hooks are `local`.** Every community pre-commit repo for Rust
works by provisioning its own environment to install rustfmt or clippy into —
a second copy of the toolchain next to the one `rust-toolchain.toml` already
installs, and a hook that fails on a machine with no network. These call the
tools already on your `PATH`, so the hook, the Docker image and CI all run the
identical binaries.

**Why `fmt` only touches staged files.** `cargo fmt --all` rewrites the whole
workspace, so an unrelated unformatted file elsewhere in the tree would
silently join your commit. That is how a formatting change ends up touching
60 files in a PR about something else.

**Running the slow checks deliberately:**

```bash
pre-commit run --all-files
pre-commit run clippy --all-files
```

## Continuous Integration

Every push and pull request runs `.github/workflows/ci.yml` (fmt, clippy, test,
WASM build, coverage) and `.github/workflows/security.yml` (cargo-audit,
cargo-deny, the security lint). PRs that touch `sdk/bindings` also run
`.github/workflows/typescript-ci.yml`.

Run the same gates locally before pushing:

```bash
make fmt lint test          # or the cargo commands directly
./scripts/security_lint.sh
(cd sdk/bindings && npm run check)
```

`./scripts/security_lint.sh --report` lists every finding with the
`SA-*` / `T-*` / `KL-*` id from `docs/SECURITY_REVIEW_CHECKLIST.md` that it
enforces.

## Before You Deploy

`make preflight` is the gate. It runs the workspace gates, builds the WASM,
checks the required environment variables, probes the target network, and
verifies the deploy configuration:

```bash
export STELLAR_DEPLOYER_IDENTITY=deployer
export STELLAR_DEPLOYER_SECRET=...        # from a secret manager
make preflight                            # full gate — use this one
make preflight-env                        # environment + network only
./scripts/preflight.sh --help
```

`scripts/deploy/deploy.sh` runs the same preflight itself before it builds or
signs anything. A deploy cannot reach a signed transaction without having
passed the gate, unless you pass `--skip-preflight`, which says so loudly in
the output.

## PR Guidelines

- Every PR must include unit tests for new functionality
- All PRs must pass `cargo clippy -- -D warnings`
- Use conventional commit messages: `feat:`, `fix:`, `chore:`, `docs:`, `test:`
- One issue per PR — never combine multiple issues

## Security Checklist

- Never commit private keys or secret values
- Always use `require_auth` for privileged operations
- Follow the CEI pattern (Checks-Effects-Interactions)
- Validate all inputs before state changes

## Resources

- [Soroban Documentation](https://developers.stellar.org/docs/smart-contracts)
- [Soroban SDK](https://docs.rs/soroban-sdk)
