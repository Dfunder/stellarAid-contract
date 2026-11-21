#!/bin/bash
# fmt_check.sh - Run cargo fmt on all contracts
# Closes #567: Run cargo fmt on all contracts

set +e

echo "Running cargo fmt check on all contracts..."

# Check formatting without making changes
cargo fmt --all -- --check

if [ $? -eq 0 ]; then
    echo "All files are properly formatted."
else
    echo "Formatting issues found. Run 'cargo fmt --all' to fix them."
    exit 1
fi