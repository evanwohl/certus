#!/bin/bash
set -e

# Certus Deployment Script for Arbitrum

NETWORK=${1:-testnet}

echo "========================================="
echo "  Certus Deployment Script"
echo "========================================="
echo "Network: $NETWORK"
echo ""

# Load environment variables
if [ -f ".env.$NETWORK" ]; then
    echo "Loading environment from .env.$NETWORK"
    source .env.$NETWORK
else
    echo "Error: .env.$NETWORK not found"
    exit 1
fi

# Validate required environment variables
if [ -z "$RPC_URL" ] || [ -z "$DEPLOYER_PRIVATE_KEY" ]; then
    echo "Error: RPC_URL and DEPLOYER_PRIVATE_KEY must be set in .env.$NETWORK"
    exit 1
fi

echo "RPC URL: $RPC_URL"
echo "Deployer: $(cast wallet address $DEPLOYER_PRIVATE_KEY)"
echo ""

# Check balance
echo "Checking deployer balance..."
BALANCE=$(cast balance $(cast wallet address $DEPLOYER_PRIVATE_KEY) --rpc-url $RPC_URL)
echo "Balance: $BALANCE wei"

if [ "$BALANCE" == "0" ]; then
    echo "Warning: Deployer has zero balance. Please fund the account."
    exit 1
fi

# Build contracts
echo ""
echo "Building contracts..."
cd contracts
forge build

# Deploy CertusEscrow
echo ""
echo "Deploying CertusEscrow..."
CONTRACT_ADDRESS=$(forge create CertusEscrow \
    --rpc-url $RPC_URL \
    --private-key $DEPLOYER_PRIVATE_KEY \
    --json | jq -r '.deployedTo')

if [ -z "$CONTRACT_ADDRESS" ] || [ "$CONTRACT_ADDRESS" == "null" ]; then
    echo "Error: Contract deployment failed"
    exit 1
fi

echo "✓ CertusEscrow deployed at: $CONTRACT_ADDRESS"

# Save contract address
echo "CERTUS_CONTRACT=$CONTRACT_ADDRESS" > ../deployed-addresses.$NETWORK.env
echo "Saved contract address to deployed-addresses.$NETWORK.env"

# Verify contract (if ETHERSCAN_API_KEY is set)
if [ -n "$ETHERSCAN_API_KEY" ]; then
    echo ""
    echo "Verifying contract on block explorer..."
    forge verify-contract $CONTRACT_ADDRESS CertusEscrow \
        --rpc-url $RPC_URL \
        --etherscan-api-key $ETHERSCAN_API_KEY \
        --watch || echo "Warning: Verification failed (contract may already be verified)"
fi

# Register test Wasm module (optional)
if [ "$NETWORK" == "testnet" ]; then
    echo ""
    echo "Registering test Wasm module..."

    # Create minimal test wasm
    TEST_WASM="0x0061736d01000000"  # \0asm version 1

    REGISTER_TX=$(cast send $CONTRACT_ADDRESS \
        "registerWasm(bytes)" $TEST_WASM \
        --rpc-url $RPC_URL \
        --private-key $DEPLOYER_PRIVATE_KEY \
        --json | jq -r '.transactionHash')

    echo "Test Wasm registered: tx $REGISTER_TX"
fi

echo ""
echo "========================================="
echo "  Deployment Complete!"
echo "========================================="
echo "Contract: $CONTRACT_ADDRESS"
echo "Network: $NETWORK"
echo "RPC: $RPC_URL"
echo ""
echo "Next steps:"
echo "1. Source the environment: source deployed-addresses.$NETWORK.env"
echo "2. Start executor: cargo run --bin executor -- $RPC_URL <private_key> $CONTRACT_ADDRESS"
echo "3. Start verifier: cargo run --bin verifier -- $RPC_URL <private_key> $CONTRACT_ADDRESS"
echo ""
