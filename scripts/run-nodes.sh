#!/bin/bash
set -e

# Run executor and verifier nodes

if [ -z "$RPC_URL" ] || [ -z "$EXECUTOR_KEY" ] || [ -z "$VERIFIER_KEY" ] || [ -z "$CONTRACT_ADDRESS" ]; then
    echo "Error: Required environment variables not set"
    echo "Set: RPC_URL, EXECUTOR_KEY, VERIFIER_KEY, CONTRACT_ADDRESS"
    exit 1
fi

echo "========================================="
echo "  Starting Certus Nodes"
echo "========================================="
echo "RPC: $RPC_URL"
echo "Contract: $CONTRACT_ADDRESS"
echo ""

# Start executor in background
echo "Starting executor node..."
./target/release/executor "$RPC_URL" "$EXECUTOR_KEY" "$CONTRACT_ADDRESS" &
EXECUTOR_PID=$!
echo "Executor PID: $EXECUTOR_PID"

# Start verifier in background
echo "Starting verifier node..."
./target/release/verifier "$RPC_URL" "$VERIFIER_KEY" "$CONTRACT_ADDRESS" &
VERIFIER_PID=$!
echo "Verifier PID: $VERIFIER_PID"

echo ""
echo "Nodes running. Press Ctrl+C to stop."

# Wait and handle shutdown
trap "echo 'Shutting down...'; kill $EXECUTOR_PID $VERIFIER_PID 2>/dev/null; exit" INT TERM

wait