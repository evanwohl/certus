#!/bin/bash

# Build all Rust nodes
cargo build --release

# Build paths
EXECUTOR="target/release/executor"
VERIFIER="target/release/verifier"

echo "Build complete:"
echo "  Executor: $EXECUTOR"
echo "  Verifier: $VERIFIER"

echo ""
echo "Usage:"
echo "  $EXECUTOR <rpc_url> <private_key> <contract_address>"
echo "  $VERIFIER <rpc_url> <private_key> <contract_address>"