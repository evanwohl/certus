#!/bin/bash
set -e

# Submit a test job to Certus

WASM_FILE=${1:-"testvectors/sha256.wasm"}
INPUT_FILE=${2:-"testvectors/input1.bin"}
PAYMENT=${3:-"50000000"} # 50 USDC

echo "========================================="
echo "  Submitting Job to Certus"
echo "========================================="

# Check env
if [ -z "$CONTRACT_ADDRESS" ] || [ -z "$RPC_URL" ] || [ -z "$CLIENT_KEY" ]; then
    echo "Error: Set CONTRACT_ADDRESS, RPC_URL, and CLIENT_KEY"
    exit 1
fi

# Read files
echo "Reading Wasm module: $WASM_FILE"
WASM_HEX=$(xxd -p -c 999999 < "$WASM_FILE")
WASM_HASH=$(sha256sum "$WASM_FILE" | cut -d' ' -f1)

echo "Reading input: $INPUT_FILE"
INPUT_HEX=$(xxd -p -c 999999 < "$INPUT_FILE")
INPUT_HASH=$(sha256sum "$INPUT_FILE" | cut -d' ' -f1)

echo ""
echo "Job details:"
echo "- Wasm hash: 0x$WASM_HASH"
echo "- Input hash: 0x$INPUT_HASH"
echo "- Payment: $PAYMENT (6 decimals)"

# USDC address on Arbitrum
USDC_ADDRESS="0xaf88d065e77c8cC2239327C5EDb3A432268e5831"

# Submit job via cast
echo ""
echo "Submitting transaction..."
TX_HASH=$(cast send "$CONTRACT_ADDRESS" \
    "createJob(bytes32,bytes32,address,uint256)" \
    "0x$WASM_HASH" \
    "0x$INPUT_HASH" \
    "$USDC_ADDRESS" \
    "$PAYMENT" \
    --rpc-url "$RPC_URL" \
    --private-key "$CLIENT_KEY" \
    --json | jq -r '.transactionHash')

echo "Transaction: $TX_HASH"

# Wait for confirmation
echo "Waiting for confirmation..."
cast receipt "$TX_HASH" --rpc-url "$RPC_URL" --confirmations 2

echo ""
echo "========================================="
echo "  Job Submitted Successfully!"
echo "========================================="
echo "Job ID calculation:"
echo "keccak256(wasmHash || inputHash || clientAddress || nonce)"