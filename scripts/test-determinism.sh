#!/bin/bash
set -e

# Test WebAssembly determinism across multiple runs

echo "========================================="
echo "  Testing Wasm Determinism"
echo "========================================="

cd node

# Run determinism tests
echo "Running determinism tests..."
cargo test determinism --release -- --nocapture

# Test with multiple iterations
echo ""
echo "Testing with 10 iterations..."
for i in {1..10}; do
    echo -n "Iteration $i: "
    cargo test --release determinism_multi_run --quiet && echo "✓" || echo "✗"
done

echo ""
echo "========================================="
echo "  Determinism Test Complete"
echo "========================================="