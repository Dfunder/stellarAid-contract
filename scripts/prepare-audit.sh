#!/usr/bin/env bash
set -euo pipefail

# This script prepares an audit-ready package.
# It gathers source code, documentation, test suites, and other artifacts.

# --- Constants ---
AUDIT_DIR="audit"
ARCHIVE_NAME="stellaraid-audit-package.tar.gz"

# --- Clean and Recreate Audit Directory ---
echo "Cleaning and recreating audit directory..."
rm -rf "$AUDIT_DIR"
mkdir -p "$AUDIT_DIR/src"
mkdir -p "$AUDIT_DIR/docs"
mkdir -p "$AUDIT_DIR/tests"
mkdir -p "$AUDIT_DIR/wasm"

# --- Copy Source Code ---
echo "Copying source code..."
cp -r campaign/src/* "$AUDIT_DIR/src/"
cp -r factory/src/* "$AUDIT_DIR/src/"
cp -r common/src/* "$AUDIT_DIR/src/"
cp -r crates/contracts/core/src/* "$AUDIT_DIR/src/"

# --- Copy Documentation ---
echo "Copying documentation..."
cp -r docs/* "$AUDIT_DIR/docs/"

# --- Copy Test Suite ---
echo "Copying test suite..."
cp -r campaign/src/test/* "$AUDIT_DIR/tests/"
cp -r factory/src/test/* "$AUDIT_DIR/tests/"

# --- Generate Commit Hash ---
echo "Generating commit hash..."
git rev-parse HEAD > "$AUDIT_DIR/commit-hash.txt"

# --- Placeholders for Additional Artifacts ---
echo "Creating placeholders for architecture diagram and threat model..."
touch "$AUDIT_DIR/ARCHITECTURE.md"
touch "$AUDIT_DIR/THREAT_MODEL.md"
echo "# Architecture Diagram\n\n[Insert architecture diagram here]" > "$AUDIT_DIR/ARCHITECTURE.md"
echo "# Threat Model\n\n[Insert threat model here]" > "$AUDIT_DIR/THREAT_MODEL.md"

# --- Pin Dependencies ---
echo "Pinning dependencies..."
cargo lock --lockfile "$AUDIT_DIR/Cargo.lock"

# --- Compile WASM ---
echo "Compiling WASM..."
make build-wasm
cp target/wasm32-unknown-unknown/release/*.wasm "$AUDIT_DIR/wasm/"

# --- Create Archive ---
echo "Creating audit package archive..."
tar -czf "$ARCHIVE_NAME" -C "$AUDIT_DIR" .

echo "✅ Audit package created: $ARCHIVE_NAME"