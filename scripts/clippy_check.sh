#!/bin/bash
# clippy_check.sh - Run cargo clippy and fix all warnings
# Closes #566: Run cargo clippy and fix all warnings

set +e

echo "Running cargo clippy on all contracts..."

# Run clippy with all targets and treat warnings as errors
cargo clippy --all-targets --all-features -- -D warnings

if [ $? -eq 0 ]; then
    echo "No clippy warnings found."
else
    echo "Clippy warnings detected. Please fix all warnings."
    exit 1
fi