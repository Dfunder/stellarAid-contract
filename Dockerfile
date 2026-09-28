# syntax=docker/dockerfile:1
#
# stellarAid contracts — reproducible build / test / deploy-assist image.
#
# Closes #708 (deployment pipeline and testnet validation).
# Multi-stage build optimised in #887.
#
# WHAT THIS IMAGE IS
#   A build and test environment for the Soroban workspace plus the shell
#   helpers under `scripts/deploy/`. It is NOT a long-running service and it
#   deliberately holds no credentials.
#
# HOW TO BUILD
#   docker build -t stellaraid-contracts .                  # deploy-assist (default)
#   docker build --target test     -t stellaraid:test .     # build + test
#   docker build --target artifacts -t stellaraid:wasm .     # WASM only, ~1/10 the size
#
# HOW TO RUN
#   docker run --rm -v "$PWD:/usr/src/app" stellaraid-contracts \
#       bash scripts/preflight.sh testnet
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
# STAGES
#   toolchain     rustup materialised from rust-toolchain.toml
#   deps          manifests only, so the network-bound resolve gets its own
#                 layer that ordinary source edits do not invalidate
#   build         compiles both the wasm and the host/test artefacts
#   test          the same image plus the workspace gates from CONTRIBUTING.md
#   artifacts     WASM + config only, on debian-slim, no Rust toolchain
#   deploy-assist the default; everything needed to run preflight/verify
#
# #887 — WHAT "OPTIMISED" ACTUALLY MEANS HERE
#   1. BuildKit cache mounts on CARGO_HOME and on target/. The registry/git
#      caches and the compiled objects are stored in a separate cache, not in
#      a layer, so a rebuild no longer ships ~1.5 GB of dependency source and
#      intermediate objects to the daemon — and, more importantly, a source
#      edit no longer invalidates the compile cache. This is the single
#      largest build-time win and it costs nothing in image size, which is
#      why it is safe to enable unconditionally.
#   2. `COPY --link`, so a layer is not invalidated by changes in unrelated
#      later layers.
#   3. Release profile tightening: symbols stripped, panic=abort. Both reduce
#      artifact size and reduce build time (less codegen, no unwinding
#      tables). `lto` and `codegen-units=1` are exposed as build ARGs but
#      default OFF: on 24 contracts they multiply build time for a marginal
#      WASM size win, and #887 asks for build time to come down. Turn them on
#      for a release build with:
#         docker build --build-arg CARGO_LTO=true \
#                      --build-arg CARGO_CODEGEN_UNITS=1 -t stellaraid:wasm .
#   4. An `artifacts` stage for the WASM-only image, which is the stage that
#      actually minimises final size. `deploy-assist` stays the default so
#      every command documented above keeps working unchanged — a smaller
#      default that cannot run `scripts/preflight.sh` (no cargo) would have
#      been a size win that broke the pipeline.
#
# NOT DONE, DELIBERATELY
#   * No `--squash` / no distroless runtime. The deploy scripts need bash, awk,
#     grep, python3 and the soroban CLI; a distroless image cannot run them,
#     and the runtime surface has to stay reviewable for a deployment tool.
#   * No vendored dependencies (`cargo vendor`). It would make the build
#     hermetic, but it adds a large generated tree to the repository and a
#     second place for dependency versions to live. `--locked` plus a
#     committed Cargo.lock is the reproducibility guarantee this repo has
#     chosen; see the NOTE in the root Cargo.toml.
#
# REQUIRES BUILDKIT
#   The `RUN --mount=type=cache` directives are BuildKit syntax and will be a
#   parse error on the legacy builder. Any Docker from 23.0 onwards, or
#   Buildx, or Docker Desktop, uses BuildKit by default; on older daemons set
#   DOCKER_BUILDKIT=1. The first line pins the frontend version so the syntax
#   does not silently depend on the local daemon.

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
#
# The cache mount holds the downloaded sources across builds. It is mounted
# `sharing=locked` so two concurrent builds cannot corrupt each other's
# registry, and it is a cache rather than a layer so the ~1.5 GB of crate
# source never reaches the daemon or an image layer.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git,sharing=locked \
    set -eux; \
    cargo fetch --locked

# ── Stage 3: build ──────────────────────────────────────────────────────────
FROM deps AS build

WORKDIR /usr/src/app

# Release profile. Set once, inherited by every cargo invocation below, so the
# wasm and host builds cannot drift apart.
#
#   strip=symbols  — debug info is dead weight in a deployed WASM.
#   panic=abort    — no unwinding tables, smaller and faster to build.
#   lto / codegen-units are opt-in; see the #887 note in the header.
ARG CARGO_STRIP=symbols
ARG CARGO_PANIC=abort
ARG CARGO_LTO=false
ARG CARGO_CODEGEN_UNITS=16

ENV CARGO_PROFILE_RELEASE_STRIP="${CARGO_STRIP}" \
    CARGO_PROFILE_RELEASE_PANIC="${CARGO_PANIC}" \
    CARGO_PROFILE_RELEASE_LTO="${CARGO_LTO}" \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_CODEGEN_UNITS}"

# `--link` lets this layer be built independently of any layer after it, so
# editing a file late in the tree does not invalidate the earlier copies.
COPY --link . .

# Two artefacts, matching the two ways this workspace is consumed:
#   * `target/wasm32-unknown-unknown/release/*.wasm` — what scripts/deploy ships.
#   * the default `target/debug` build — what the integration tests link against.
#
# `--locked` is repeated here so a lockfile that disagrees with the manifests
# fails here too, not only in `deps`.
#
# WHY THE ARTIFACTS ARE COPIED OUT
#   The `target` cache mount is ephemeral: nothing written under it is part of
#   the image filesystem. Copying the results into build-artifacts/ inside the
#   same RUN is what makes them visible to the later stages — without this,
#   `COPY --from=build target/...` would silently copy an empty directory and
#   the image would build "successfully" with no contracts in it.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=cargo-target,target=/usr/src/app/target,sharing=locked \
    set -eux; \
    cargo build --locked --release --target wasm32-unknown-unknown --workspace; \
    cargo build --locked --workspace --tests; \
    out=target/wasm32-unknown-unknown/release; \
    count="$(find "$out" -maxdepth 1 -name '*.wasm' | wc -l)"; \
    echo "wasm artifacts produced: $count"; \
    if [ "$count" -eq 0 ]; then \
        echo "the release build produced no .wasm in $out" >&2; \
        exit 1; \
    fi; \
    mkdir -p build-artifacts/wasm; \
    cp "$out"/*.wasm build-artifacts/wasm/; \
    ls -la build-artifacts/wasm/

# ── Stage 4: test ───────────────────────────────────────────────────────────
# Same image, same sources, plus the workspace gates from CONTRIBUTING.md.
FROM build AS test

# Reports are mounted/written here by the CI job, not into the image.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=cargo-target,target=/usr/src/app/target,sharing=locked \
    set -eux; \
    cargo fmt --all -- --check; \
    cargo clippy --locked --all-targets -- -D warnings; \
    cargo test --locked --workspace --no-fail-fast

# ── Stage 5: artifacts ──────────────────────────────────────────────────────
# The size-minimal image: the compiled contracts and a checksum manifest, and
# nothing else. No Rust toolchain, no sources, no cargo registry — none of
# which can be useful once the WASM exists.
#
# Intended for inspecting, hashing and publishing a release build:
#   docker build --target artifacts -t stellaraid:wasm .
#   docker run --rm stellaraid:wasm ls -la /wasm
#   docker run --rm stellaraid:wasm cat /wasm/MANIFEST.sha256
#
# NOT intended for running the deploy scripts: preflight.sh and deploy.sh
# shell out to cargo and the soroban CLI, which are not present here. Use
# `deploy-assist` for those.
FROM debian:bookworm-slim AS artifacts

# A non-root user, because an image extracted by an automated pipeline should
# not default to uid 0 even when it only holds public artefacts.
RUN set -eux; \
    useradd --create-home --uid 10001 starlaraid

WORKDIR /wasm

# Sourced from build-artifacts/, never from target/: target/ lives in an
# ephemeral cache mount and would be empty here.
COPY --from=build --chown=starlaraid:starlaraid /usr/src/app/build-artifacts/wasm/ /wasm/

# A self-describing manifest, generated rather than hand-written so it cannot
# drift from what was actually produced.
RUN set -eux; \
    { \
      echo "repository: Dfunder/stellarAid-contract"; \
      echo "built:       $(date -u +%Y-%m-%dT%H:%M:%SZ)"; \
      echo "artifacts:   $(ls -1 /wasm/*.wasm | wc -l)"; \
      echo; \
      sha256sum /wasm/*.wasm; \
    } > /wasm/MANIFEST.sha256; \
    chmod 0444 /wasm/MANIFEST.sha256; \
    cat /wasm/MANIFEST.sha256

USER starlaraid

CMD ["ls", "-la", "/wasm"]

# ── Stage 6: deploy-assist (default) ────────────────────────────────────────
# Carries the compiled WASM and the scripts, so an operator can run
# preflight / verify from a container that matches the build.
FROM build AS deploy-assist

WORKDIR /usr/src/app

# Restore the WASM to the path scripts/deploy/lib.sh:wasm_path() reads
# (target/wasm32-unknown-unknown/release/<crate>.wasm). It has to be put back
# because the build stage's target/ was an ephemeral cache mount. Without this
# the container would report every contract as missing and the documented
# `bash scripts/preflight.sh testnet` would fail on a perfectly good build.
RUN set -eux; \
    mkdir -p target/wasm32-unknown-unknown/release; \
    cp build-artifacts/wasm/*.wasm target/wasm32-unknown-unknown/release/; \
    ls -1 target/wasm32-unknown-unknown/release/*.wasm | wc -l

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
CMD ["scripts/preflight.sh", "testnet"]
