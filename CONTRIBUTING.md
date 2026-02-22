# Contributing to StellarAid

We're excited that you want to contribute to StellarAid! This document provides guidelines and instructions for contributing to the project.

## Getting Started

### Prerequisites

- Rust stable (automatically managed via `rust-toolchain.toml`)
- Cargo (comes with Rust)
- Git

### Setup Development Environment

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/stellaraid-contract.git
cd stellaraid-contract

# Quick setup (installs dependencies, builds everything)
make setup

# Or manually:
rustup update  # Update Rust
make install-tools  # Install Soroban CLI and required targets
make build  # Build the project
```

## Code Quality Standards

We enforce code quality through **rustfmt**, **clippy**, and optional **pre-commit hooks**. These standards ensure consistency and catch common mistakes.

### 1. Code Formatting with rustfmt

We use [rustfmt](https://rust-lang.github.io/rustfmt/) to maintain consistent code style. The project includes a `rustfmt.toml` with formatting rules.

**Check formatting (fails if code is not formatted):**
```bash
cargo fmt --all -- --check
```

**Auto-format your code:**
```bash
cargo fmt --all
```

Or use the Makefile:
```bash
make fmt
```

**IDE Integration:**
- **VS Code**: Install the "Rust Analyzer" extension and enable `[editor.formatOnSave]`
- **IntelliJ IDEA**: Enable "Reformat code on Save" in Settings
- **Vim/Neovim**: Configure with `rustfmt` as the default formatter

### 2. Linting with Clippy

We use [Clippy](https://github.com/rust-lang/rust-clippy) as a strict linter to catch common pitfalls and improve code quality.

**Run clippy with strict settings:**
```bash
cargo clippy --workspace -- -D warnings
```

Or use the Makefile:
```bash
make lint
```

**Clippy will deny:**
- All standard clippy warnings (`-D warnings`)
- Common correctness issues
- Performance anti-patterns
- Code complexity problems
- Readability issues

If clippy reports warnings, fix them before submitting your PR. In rare cases where clippy gives false positives, you can suppress specific warnings with:

```rust
#[allow(clippy::warning_name)]
```

Include a comment explaining why the warning is suppressed.

### 3. Running Tests

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p stellaraid-core

# Run with output
cargo test --workspace -- --nocapture
```

## Recommended Workflow

### Option A: Manual Quality Checks (Minimum)

Before committing code:

```bash
# 1. Format code
cargo fmt --all

# 2. Run linter
cargo clippy --workspace -- -D warnings

# 3. Run tests
cargo test --workspace
```

Or use the combined Makefile command:
```bash
make fmt lint test
```

### Option B: Automated Checks with Pre-commit Hooks (Optional but Recommended)

For automatic code quality checks on every commit:

#### Installation

1. **Install pre-commit:**
   ```bash
   # macOS/Linux
   pip install pre-commit
   
   # Windows (using pip or pip3)
   pip install pre-commit
   ```

2. **Enable pre-commit hooks in your repo:**
   ```bash
   pre-commit install
   ```

3. **Verify installation:**
   ```bash
   pre-commit run --all-files
   ```

#### How It Works

Once enabled, pre-commit hooks will automatically run on `git commit`:

- **On commit**: Runs rustfmt check and clippy
- **On push**: Runs the full test suite (optional, can be slower)

If any check fails, commit is aborted. Fix the issues and try again:

```bash
# Fix formatting
cargo fmt --all

# Fix clippy warnings
# (Edit code manually to resolve linter warnings)

# Retry commit
git add .
git commit -m "Your message"
```

#### Temporarily Skip Checks (Last Resort)

```bash
# Skip pre-commit hooks for a single commit
git commit --no-verify

# Disable all pre-commit hooks
pre-commit uninstall

# Re-enable pre-commit hooks
pre-commit install
```

## Submission Guidelines

### Before Submitting a Pull Request

1. **Code Quality**: Ensure all checks pass
   ```bash
   cargo fmt --all -- --check  # Format check
   cargo clippy --workspace -- -D warnings  # Linter
   cargo test --workspace  # Tests
   ```

2. **Commit Messages**: Use clear, descriptive messages
   ```
   [feature] Add new crowdfunding tier system
   [fix] Resolve panic in donation calculation
   [docs] Update API documentation
   [test] Add tests for refund logic
   ```

3. **Tests**: Add tests for new features and bug fixes

4. **Documentation**: Update README.md, rustdoc comments, and this file if needed

### Pull Request Checklist

- [ ] Code is formatted (`cargo fmt --all`)
- [ ] Clippy passes with `-D warnings`
- [ ] All tests pass (`cargo test --workspace`)
- [ ] Tests added for new features
- [ ] Documentation updated
- [ ] Commit messages are clear and descriptive
- [ ] No hardcoded values or debug prints

## Project Structure

```
stellarAid-contract/
├── Cargo.toml                    # Workspace configuration
├── Makefile                      # Development commands
├── rustfmt.toml                  # Code formatting rules
├── rust-toolchain.toml           # Rust version & components
├── .pre-commit-config.yaml       # Optional pre-commit hooks
├── CONTRIBUTING.md               # This file
├── README.md                     # Project overview
├── crates/
│   ├── contracts/
│   │   └── core/                 # Smart contract implementation
│   │       ├── src/lib.rs        # Contract code
│   │       └── tests/            # Contract tests
│   └── tools/                    # CLI tools and utilities
│       └── src/main.rs           # CLI entry point
└── target/                       # Build artifacts (ignored)
```

## Key Files

- **rustfmt.toml**: Formatting configuration (max width 100 chars, etc.)
- **rust-toolchain.toml**: Enforces stable Rust with rustfmt and clippy
- **.pre-commit-config.yaml**: Optional git hooks for automatic checks
- **Makefile**: Quick commands for build, test, fmt, lint

## Development Tips

### Useful Commands

```bash
# Check everything (format without modifying)
cargo fmt --all -- --check

# Check clippy with details
cargo clippy --workspace --verbose -- -D warnings

# Test a specific file
cargo test --lib contract::tests

# Build with verbose output
cargo build --verbose

# Generate and open documentation
cargo doc --open

# Check for security issues
cargo install cargo-audit
cargo audit
```

### Common Issues

**Q: Clippy warns about a pattern I want to use**
- Run clippy with `--explain` to understand the issue
- In rare cases, suppress with `#[allow(clippy::lint_name)]` with a comment

**Q: Why do some imports get reordered?**
- rustfmt automatically organizes imports for consistency

**Q: Can I use different formatting rules?**
- No, all contributors use the same `rustfmt.toml` for consistency

**Q: Pre-commit hooks are too slow**
- You can modify `.pre-commit-config.yaml` to run clippy only on push
- Or disable and use `make fmt lint` manually

## Reporting Issues

Found a bug or have a suggestion? Please open an issue with:
- Clear description of the problem
- Steps to reproduce (if applicable)
- Expected vs actual behavior
- Your Rust version (`rustc --version`)

## Code Review Process

1. Automated checks run (GitHub Actions)
2. Manual code review by maintainers
3. Address feedback and iterate
4. Merge when approved

## Questions?

- Check the [README.md](README.md) for project overview
- Open an issue for clarification
- Contact maintainers in discussions

---

**Thank you for contributing to StellarAid!** 🌟

Happy coding!
