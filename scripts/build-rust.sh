#!/bin/bash
set -e

# Build Rust nodes with optimizations

echo "========================================="
echo "  Building Certus Rust Nodes"
echo "========================================="

# Build in release mode
echo "Building executor..."
cargo build --release --bin executor

echo "Building verifier..."
cargo build --release --bin verifier

# Run tests
echo ""
echo "Running tests..."
cargo test --all

# Check determinism
echo ""
echo "Checking Wasm determinism..."
cargo test --package certus-executor determinism

echo ""
echo "========================================="
echo "  Build Complete!"
echo "========================================="
echo ""
echo "Binaries located at:"
echo "- target/release/executor"
echo "- target/release/verifier"
echo ""