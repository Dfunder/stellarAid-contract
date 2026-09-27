# WASM Size Optimization

> Closes **#874** — add contract size optimization documentation.
>
> Related: [WASM.md](./WASM.md) (how to build), [PERFORMANCE_TARGETS.md](./PERFORMANCE_TARGETS.md)
> (per-invocation cost targets), [UPGRADE_AND_ROLLBACK.md](./UPGRADE_AND_ROLLBACK.md) (size check
> before an upgrade), [QUERY_OPTIMIZATION.md](./QUERY_OPTIMIZATION.md) (bounding *reads*, which is a
> different limit from the one here).

Soroban contracts are deployed as WebAssembly, and the network refuses to install a contract whose
WASM exceeds a protocol-level size limit. This document covers what that limit is, how to measure
where this repository currently sits against it, and which levers actually move the number.

---

## 1. The limits, and which one people mean

There are three separate size limits in play, and conflating them is the most common source of
"the contract is too big" confusion. Only the first is about the WASM file.

| Limit | Applies to | Mainnet value | Where it bites |
|---|---|---|---|
| **Contract code entry** | The deployed `.wasm` | `max_contract_code_entry_size_bytes` | Deployment is rejected outright. |
| **Contract data entry** | One ledger entry of contract state | `max_contract_data_entry_size_bytes` | One oversized stored value is rejected. |
| **Contract data key** | One storage key | `max_contract_data_key_size_bytes` | A key built from unbounded caller input. |

The authoritative source is the SDK, not this document. `soroban-sdk` exposes the current mainnet
values as constants, and the usage below is the SDK's own documented form:

```rust
use soroban_sdk::testutils::cost_estimate::NetworkInvocationResourceLimits;
use soroban_env_host::InvocationResourceLimits;

let limits = InvocationResourceLimits::mainnet();
```

As shipped in `soroban-sdk` 25.1.1 those constants are:

| Constant | Value | Bytes |
|---|---|---|
| `max_contract_code_entry_size_bytes` | 131072 | 128 KiB |
| `max_contract_data_entry_size_bytes` | 65536 | 64 KiB |
| `max_contract_data_key_size_bytes` | 250 | — |
| `contract_events_size_bytes` | 16384 | 16 KiB |

The same struct also carries the per-invocation ceilings that
[`PERFORMANCE_TARGETS.md`](./PERFORMANCE_TARGETS.md) budgets against. They are listed here because a
contract can sit comfortably inside the size limit and still fail a transaction on these:

| Constant | Value |
|---|---|
| `instructions` | 600,000,000 |
| `mem_bytes` | 41,943,040 |
| `disk_read_entries` | 100 |
| `write_entries` | 50 |
| `ledger_entries` | 100 |
| `disk_read_bytes` | 200,000 |
| `write_bytes` | 132,096 |

Note what `write_entries = 50` and `write_bytes = 132,096` imply for this repository specifically:
one invocation rewriting a large `Vec` as a single ledger entry can blow the **byte** budget long
before it blows the **entry** budget, because it is one entry that is very large. This is the single
most actionable line in the table, and §5 returns to it.

**Read that table, do not memorise it.** These values track the protocol the SDK was built against,
not the protocol your target network is currently running, and the code-entry limit in particular
has been raised between protocol releases. This repository pins `soroban-sdk = "21.0.0"`, which
targets an earlier protocol where the code-entry limit was lower than the 128 KiB above. Before
relying on any headroom figure here, check the two things that actually decide it:

1. The `soroban-sdk` version in the root `Cargo.toml` (`soroban-sdk = "21.0.0"` today).
2. The protocol version of the network you are deploying to, which is what the numbers above are
   actually enforced against:

   ```bash
   curl -s -X POST "$STELLAR_RPC_URL" \
     -H 'Content-Type: application/json' \
     -d '{"jsonrpc":"2.0","id":1,"method":"get_ledger_info","params":{}}' \
     | jq -r .result.protocol_version
   ```

For that reason this repository does **not** treat 128 KiB as its budget. See §4.

---

## 2. The repo's own budget

Rather than track a number that moves with every SDK bump, this repository adopts a fixed working
budget that stays under the *lowest* limit any plausible protocol version imposes.

| Threshold | Action |
|---|---|
| **≤ 48 KiB** | Green. Expected for most contracts here. |
| **48–56 KiB** | Warn. Worth a look at the diff before merging. |
| **56–64 KiB** | Fail. A contract this size is close to the historical 64 KiB limit and must be split or shrunk before it ships. |
| **> 64 KiB** | Deployment will fail on any protocol with the 64 KiB code-entry limit. Treat as a blocker. |

The 64 KiB line is the one that matters: it is the limit this repository's pinned SDK was written
against, so anything under it deploys everywhere. 48 KiB is a deliberately conservative green
threshold — it leaves room for a dependency bump that grows the binary without anyone having to
re-derive the policy.

This is a **policy, not a measurement.** No contract in this repository has been built, so no
contract has been measured against it. See §6.

---

## 3. Measuring contract size

### 3.1 Build the WASM

```bash
cargo build --target wasm32-unknown-unknown --release
```

The artifacts land in `target/wasm32-unknown-unknown/release/*.wasm`. `rust-toolchain.toml` already
pins the `wasm32-unknown-unknown` target, so no `rustup target add` is needed.

**The file on disk is what gets uploaded and hashed.** The ledger does not re-link or strip it, so
the byte count of that file *is* the number the network checks. Measure that file, not a
post-processed copy, unless you also post-process before deploying.

### 3.2 Print a table

```bash
cargo build --target wasm32-unknown-unknown --release
for w in target/wasm32-unknown-unknown/release/*.wasm; do
  bytes=$(wc -c < "$w")
  printf '%-40s %8d bytes  %6.1f KiB\n' \
    "$(basename "$w")" "$bytes" "$(echo "$bytes / 1024" | bc -l)"
done | sort -k2 -n -r
```

To check the policy rather than just print sizes:

```bash
cargo build --target wasm32-unknown-unknown --release
fail=0
for w in target/wasm32-unknown-unknown/release/*.wasm; do
  kib=$(( $(wc -c < "$w") / 1024 ))
  if   [ "$kib" -gt 64 ]; then echo "FAIL  $(basename "$w"): ${kib} KiB"; fail=1
  elif [ "$kib" -gt 56 ]; then echo "FAIL  $(basename "$w"): ${kib} KiB"; fail=1
  elif [ "$kib" -gt 48 ]; then echo "WARN  $(basename "$w"): ${kib} KiB"
  else echo "ok    $(basename "$w"): ${kib} KiB"; fi
done
exit $fail
```

The thresholds here are exactly the §2 policy table, and note that the two FAIL branches are
deliberate: §2 treats 56–64 KiB as a must-fix-before-shipping and >64 KiB as a guaranteed
deployment rejection, so both must fail the check rather than merely warn.

(There is no `make wasm-size` target. `Makefile` has `build`, which is
`cargo build --target wasm32-unknown-unknown --release`; adding a size target to the Makefile is
tracked separately so it does not get entangled with the release-profile question in §5.1.)

### 3.3 Finding out what is taking up the space

Once you know a contract is too big, you need to know *why*:

```bash
# Top-level function bodies by size.
wasm-objdump -x target/wasm32-unknown-unknown/release/<contract>.wasm | less

# After wasm-opt, the module is far easier to read.
wasm-opt -Oz --print-function-map <contract>.wasm
```

Custom sections are usually the cheapest win. `wasm-objdump -h` will show `.debug_*`, `.name` and
friends; the release profile's `strip = true` removes them, which is why §5.1 matters.

### 3.4 Verifying against the network

The authoritative check is an upload. Deploy to testnet and let the network judge:

```bash
stellar contract install --wasm <contract>.wasm --source-account deployer --network testnet
```

A code-entry limit violation is rejected at install time, not at invoke time, so a testnet install
is a complete size check. Do this as part of preflight rather than treating it as a surprise on
mainnet.

---

## 4. Optimisation techniques

Ordered roughly by how much they actually move the number, biggest first.

### 4.1 Use the release profile (biggest lever, currently not applied)

`Cargo.toml.optimization` in the repo root already contains the right settings:

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

**None of this is active.** That file is a scratch note, not a manifest Cargo reads — the root
`Cargo.toml` has no `[profile.release]` section at all, so `make build` and `cargo build --release`
both use Cargo's default release profile (`opt-level = 3`, no LTO, debug info retained). Every
contract in this repository is therefore being built *unoptimised*, and every size measurement taken
today is measuring the wrong artifact.

`opt-level = "z"` plus `lto = true` plus `codegen-units = 1` is typically the single largest
reduction available for a Soroban contract, often 40–60% off a default-profile build. `strip = true`
removes the DWARF and `.name` custom sections, which are pure overhead.

Adopting it is a one-block change to the root `Cargo.toml`, and it is deliberately **not** made
here: it changes the build output of all 27 workspace members at once, materially increases release
build time, and cannot be validated in this repository's current state (see §6). It is the highest
value follow-up to this document.

### 4.2 `no_std`

Every contract in `contracts/*/src/lib.rs` already opens with `#![no_std]`, which is the single
most important line for size. `std` drags in the allocator, formatting machinery, panic runtime
support and the collection ecosystem, and the Soroban VM provides none of it. The one thing to
watch is accidental leakage: a stray `extern crate std;`, or a dev-dependency pulled into the
non-dev dependency graph, reintroduces all of it. `shared` keeps its `extern crate std;` inside
`#[cfg(test)] mod tests` for exactly this reason.

### 4.3 `symbol_short!` instead of `symbol!`

Contracts build event topics with `symbol_short!("submitted")` rather than `symbol!("submitted")`.
`symbol_short!` is enforced at compile time to be ≤ 9 characters, and a `symbol!` longer than that
is a runtime panic — so `symbol_short!` converts a class of runtime failure into a build failure.

Be precise about the size claim, though: **both compile to the same 4-byte `Val::Symbol` payload, so
switching to `symbol_short!` does not meaningfully shrink the binary.** The win is compile-time
safety and marginally cheaper codegen, not bytes. Symbols are already as cheap as a 32-bit handle
can be; there is no further compression available at this layer.

### 4.4 `wasm-opt` as a post-link pass

Binaryen can shrink further after linking, because it sees the final module and can drop
unreachable functions and fold across crate boundaries that LTO left alone:

```bash
wasm-opt -Oz input.wasm -o output-optimized.wasm
```

`-Oz` optimises for size, `-O2` for speed. Caveats worth knowing before adopting it:

- It must run on the exact artifact that will be deployed, or the measured size is not the deployed
  size.
- It is a second toolchain dependency that CI would need pinned.
- It occasionally interacts badly with Soroban's host imports. Always re-run the contract's test
  suite against the optimised artifact, not just the original.

### 4.5 Keep formatting machinery out of the WASM

`core::fmt` is the quiet size hog. Any reachable `format!`, `write!`, or `{:?}` pulls in the
argument-list and formatting infrastructure, which does not compress well.

`contracts/verification/src/errors.rs` implements `Display` for `VerificationError`. That impl alone
is harmless — it is dead-stripped if nothing calls it — but the moment some code path in the WASM
build calls `to_string()` on an error, the whole machinery becomes reachable and the binary grows by
more than most of the contract's own logic. Contracts should return the error enum across the
contract boundary and let the client format it, which is what `#[contracterror]` is for.

### 4.6 Pick narrow field types

Ledger entry size is a separate budget (§1) but the same discipline applies to both:

- `u32` over `u64` where the value is a count, index or ledger offset. Every contract here uses `u32`
  for these, which is correct.
- `Option<Address>` costs more than `Address` — an `Option<Address>` is 33 bytes against 20 — and
  most "absent" cases in practice are a zero address. Where the semantics allow it, prefer a
  zero-address sentinel. `verification::Portfolio.reviewer` is a case where the `Option` earns its
  cost, because `None` ("not yet reviewed") and "reviewed by the zero address" are genuinely
  different states.
- Smaller `#[contracttype]` enums are cheaper, but this is rarely the bottleneck.

### 4.7 Gate what you do not need

Feature flags and `#[cfg(not(feature = "..."))]` keep optional code paths out of the deployed
artifact. Be careful with `#[cfg(test)]`: test-only helpers must be gated, or they ship.

### 4.8 Do not expect generic or trait machinery to be free

Soroban contracts are generic-heavy at the source level (`Page<T>`, `IntoVal<Env, Val>` bounds)
because the SDK's conversion traits are generic by design. Monomorphisation is handled at the
`cdylib` boundary — only the instantiations reachable from `#[contractimpl]` entry points are
emitted — so this is usually a non-issue. It becomes a problem only if a generic function is
instantiated from many call sites with many types.

---

## 5. Where the size limit and the cost limit meet

Size (§1) and per-invocation cost (`PERFORMANCE_TARGETS.md`) are separate budgets, but two
techniques serve both, which is why they are grouped here rather than duplicated.

**Bounded list reads.** A function that returns an entire unbounded `Vec` can exceed the *data
entry* size limit and blow the invocation's read budget at the same time. `shared::pagination` (see
`contracts/shared/src/pagination.rs`) caps every page at `MAX_PAGE_SIZE = 50` entries, which keeps a
list query inside both budgets regardless of how large the underlying list has grown.

**Append-only lists are rewritten in full.** `push_history`-shaped code — read the whole list, push,
write the whole list back — is O(len) per append, in both CPU instructions and write bytes. The
verification contract trims `History` and `BadgeHistory` to `HistoryLimit` on every write for
exactly this reason. Any new append-only list needs a cap, and the benchmark in
`contracts/verification/benches/verification_benchmark.rs` reports the resulting list length as its
`state` column so the growth is visible rather than theoretical.

---

## 6. Honest status

- **No contract in this repository has been built, so no size in this document is a measurement.**
  There is no baseline table, and one is not invented here.
- **The workspace does not currently build.** Twelve files under `contracts/` have unclosed
  delimiters on `upstream/main` and cannot be parsed, including
  `contracts/reputation/src/lib.rs`, `contracts/escrow/src/lib.rs` and everything under
  `contracts/platform_config/`. This is pre-existing and unrelated to this issue; see
  `docs/DEPLOYMENT.md` §8. Every command in §3 will fail before it produces a number.
- **The thresholds in §2 are policy, chosen against the lowest plausible protocol limit, not
  derived from this repository's contracts.** They are a ceiling to tighten once real output
  exists.
- **The `soroban-sdk` values quoted in §1 are from 25.1.1**, read out of that release's source. The
  repository pins 21.0.0, whose code-entry limit is lower. Verify against the SDK version you
  actually build with.
- **§4.1's recommendation is not applied.** The release profile in `Cargo.toml.optimization` remains
  inert.
