# syntax=docker/dockerfile:1
#
# stellarAid contracts — reproducible build / test / deploy-assist image.
#
# Closes #708 (deployment pipeline and testnet validation).
#
# WHAT THIS IMAGE IS
#   A build and test environment for the Soroban workspace plus the shell
#   helpers under `scripts/deploy/`. It is NOT a long-running service and it
#   deliberately holds no credentials.
#
# HOW TO BUILD
#   docker build -t stellaraid-contracts .
#   docker build --target test -t stellaraid-contracts:test .   # build + test
#
# HOW TO RUN
#   docker run --rm -v "$PWD:/usr/src/app" stellaraid-contracts \
#       bash scripts/deploy/preflight.sh testnet
#   docker run --rm -v "$PWD:/usr/src/app" stellaraid-contracts \
#       bash scripts/deploy/verify_deploy.sh testnet
#
#   Deployment itself needs a Stellar account. Supply it at RUN time, never at
#   BUILD time, and never bake it into a layer:
#     docker run --rm -e STELLAR_DEPLOYER_SECRET -v "$PWD:/usr/src/app" \
#       stellaraid-contracts bash scripts/deploy/deploy_testnet.sh
#   (`-e NAME` forwards the value from the caller's environment without putting
#   it in the image, the container spec, or the shell history.)
#
# TOOLCHAIN
#   The base image ships rustup, so `rust-toolchain.toml` is authoritative:
#   `rustup show` installs the channel it names plus the `wasm32-unknown-unknown`
#   target and the `rustfmt` / `clippy` components. The image therefore tracks
#   `channel = "stable"` in that file rather than pinning a second version here.
#   Pinned in the repo: edition 2021, `resolver = "2"`, soroban-sdk 21.0.0.
#
# CACHE ORDER
#   toolchain -> deps (manifests only) -> build (sources) -> test.
#   Dependency resolution is the slow, network-bound part and sits in its own
#   layer that ordinary source edits do not invalidate.

# ── Stage 1: toolchain ──────────────────────────────────────────────────────
FROM rust:slim-bookworm AS toolchain

WORKDIR /usr/src/app

# rust-toolchain.toml declares: channel = "stable",
# targets = ["wasm32-unknown-unknown"], components = ["rustfmt", "clippy"].
COPY rust-toolchain.toml ./

# `rustup show` is the supported way to materialise a rust-toolchain.toml
# toolchain plus its targets and components. Fail the build here rather than
# discovering a missing wasm target halfway through a 20-crate compile.
RUN set -eux; \
    rustup show; \
    rustc --version; \
    cargo --version; \
    rustup target list --installed | grep -qx wasm32-unknown-unknown

# ── Stage 2: dependencies ───────────────────────────────────────────────────
# Manifests only. `cargo fetch` resolves the whole graph from the workspace
# manifests plus the committed Cargo.lock, so the source tree is not required —
# which lets this layer survive every ordinary source edit.
FROM toolchain AS deps

RUN set -eux; \
    mkdir -p /usr/src/app; \
    cp rust-toolchain.toml /usr/src/app/; \
    for m in Cargo.toml contracts/*/Cargo.toml sdk/Cargo.toml tests/*/Cargo.toml; do \
        mkdir -p "/usr/src/app/$(dirname "$m")"; \
        cp "$m" "/usr/src/app/$m"; \
    done; \
    cp Cargo.lock /usr/src/app/

WORKDIR /usr/src/app

# `--locked` is the reproducibility guarantee: a Cargo.lock that disagrees with
# the manifests is an error, not a silent re-resolve.
RUN cargo fetch --locked

# ── Stage 3: build ──────────────────────────────────────────────────────────
FROM deps AS build

WORKDIR /usr/src/app

# The registry/git caches written by the previous stage travel with the image,
# so this layer only compiles.
COPY . .

# Two artifacts, matching the two ways this workspace is consumed:
#   * `target/wasm32-unknown-unknown/release/*.wasm` — what scripts/deploy ships.
#   * the default `target/debug` build — what the integration tests link against.
RUN set -eux; \
    cargo build --release --target wasm32-unknown-unknown --workspace; \
    cargo build --workspace --tests

# ── Stage 4: test ───────────────────────────────────────────────────────────
# Same image, same sources, plus the workspace gates from CONTRIBUTING.md.
FROM build AS test

# Reports are mounted/written here by the CI job, not into the image.
RUN set -eux; \
    cargo fmt --all -- --check; \
    cargo clippy --all-targets -- -D warnings; \
    cargo test --workspace --no-fail-fast

# ── Stage 5: deploy-assist ──────────────────────────────────────────────────
# The default target. Carries the compiled WASM and the scripts, so an operator
# can run preflight / verify from a container that matches the build.
FROM build AS deploy-assist

WORKDIR /usr/src/app

# Fail closed if a secret was ever committed into the build context. This is a
# backstop, not a substitute for .dockerignore and .gitignore.
#
# `.git` is excluded from the context, so a `git ls-files` probe would be a
# no-op here; the two filesystem probes below are the ones that bite, and they
# cover the two places a local secret actually lives (a dotenv file and a
# Soroban identity directory).
RUN set -eux; \
    ! test -e .env; \
    ! test -e .soroban/accounts

ENV NETWORK=testnet

ENTRYPOINT ["bash"]
CMD ["scripts/deploy/preflight.sh", "testnet"]
