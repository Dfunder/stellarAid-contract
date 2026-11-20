# Environment variable reference

This is the single canonical definition of environment variables used by the
contract tooling. `.env.example` is a starting template, not a source of truth.
Never commit a real `.env` or a secret value.

| Name | Required | Used by | Default | Example | Notes / aliases |
|------|----------|---------|---------|---------|-----------------|
| `STELLAR_DEPLOYER_IDENTITY` | Testnet/mainnet deploy | `scripts/deploy/lib.sh`, `scripts/preflight.sh` | none | `deployer` | Preferred signing source: a local Soroban identity name, not a secret. |
| `STELLAR_DEPLOYER_SECRET` | Yes (presence checked) | `scripts/deploy/lib.sh`, `scripts/preflight.sh` | none | injected by a secret manager | Secret key read only from the environment and passed to the CLI by variable reference. Never a CLI argument. |
| `STELLAR_ALLOW_MAINNET` | Mainnet only | `scripts/deploy/lib.sh`, `scripts/preflight.sh` | unset | `1` | Mainnet gate. Must be exactly `1`; no default, no wildcard. |
| `STELLAR_MAINNET_CONFIRM` | Mainnet only | `scripts/deploy/lib.sh`, `scripts/preflight.sh` | none | exact mainnet passphrase copied from `.soroban/config.toml` | Intent check only; compared verbatim with the configured passphrase. |
| `SOROBAN_CONFIG` | No | `scripts/deploy/lib.sh` | `./.soroban/config.toml` | `./.soroban/config.toml` | Path override for the network registry. RPC URL and passphrase come from this file, not from environment aliases. |
| `WASM_DIR` | No | `scripts/deploy/lib.sh`, `scripts/generate_abi.sh`, `scripts/verify_bindings.sh` | `./target/wasm32-unknown-unknown/release` | same | Location of built WASM artifacts. |
| `DEPLOYMENT_MANIFEST` | No | `scripts/deploy/lib.sh` | `./scripts/deploy/contracts.toml` | same | Overrides the deployment manifest path for tests/alternate inventories. |
| `NETWORK` | No | `scripts/preflight.sh` | `testnet` | `testnet` | Preflight target network; must exist in `SOROBAN_CONFIG`. |
| `NO_COLOR` | No | `scripts/deploy/lib.sh` | unset | `1` | Any non-empty value disables colored output. |
| `ADMIN_SECRET` | Deprecated | legacy root `scripts/deploy.sh` | none | do not use | Alias of `STELLAR_DEPLOYER_SECRET`. It was historically passed as `--source "$ADMIN_SECRET"`, exposing the secret in argv. Use the identity flow above. |
| `SOROBAN_NETWORK_PASSPHRASE` | Deprecated | legacy docs/tooling | none | do not use | Alias of the passphrase stored in `.soroban/config.toml`. Do not export a passphrase. |
| `NETWORK_PASSPHRASE` | Deprecated | legacy docs/tooling | none | do not use | Alias of the same configured passphrase. |
| `STELLAR_NETWORK_PASSPHRASE` | Deprecated | legacy docs/tooling | none | do not use | Alias of the same configured passphrase. |

No deprecated passphrase alias is accepted by the current scripts. Network
identity is selected by name (`NETWORK`) and resolved through
`SOROBAN_CONFIG`, so there is exactly one source of truth for RPC URLs and
passphrases.

`CARGO_PKG_NAME` and `CARGO_PKG_VERSION` are Cargo build-time variables used by
`shared::version`; they are not operator environment variables and are
documented in [VERSIONING.md](./VERSIONING.md).

`scripts/verify_env_example.sh` fails when a `STELLAR_*`, `SOROBAN_*`,
`NO_COLOR`, or legacy alias reference appears in code/docs but is missing from
this table, or when `.env.example` defines a variable that is not in it.
