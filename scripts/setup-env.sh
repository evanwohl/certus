#!/bin/bash
set -e

# Setup development environment for Certus

echo "========================================="
echo "  Certus Environment Setup"
echo "========================================="

# Check prerequisites
echo "Checking prerequisites..."

# Check Rust
if ! command -v rustc &> /dev/null; then
    echo "✗ Rust not found. Install from https://rustup.rs"
    exit 1
else
    echo "✓ Rust $(rustc --version)"
fi

# Check Foundry
if ! command -v forge &> /dev/null; then
    echo "✗ Foundry not found. Installing..."
    curl -L https://foundry.paradigm.xyz | bash
    foundryup
else
    echo "✓ Foundry $(forge --version)"
fi

# Check Node.js (for CLI)
if ! command -v node &> /dev/null; then
    echo "⚠ Node.js not found (optional, needed for CLI)"
else
    echo "✓ Node.js $(node --version)"
fi

# Create .env from example
if [ ! -f .env ]; then
    if [ -f .env.example ]; then
        echo ""
        echo "Creating .env from .env.example..."
        cp .env.example .env
        echo "✓ Created .env (update with your keys)"
    fi
fi

# Install Rust dependencies
echo ""
echo "Installing Rust dependencies..."
cd node
cargo fetch
cd ..

# Compile contracts
echo ""
echo "Compiling smart contracts..."
cd contracts
forge build
cd ..

echo ""
echo "========================================="
echo "  Setup Complete!"
echo "========================================="
echo ""
echo "Next steps:"
echo "1. Update .env with your private keys and RPC URL"
echo "2. Run './scripts/build-rust.sh' to build nodes"
echo "3. Run './scripts/deploy.sh' to deploy contracts"
echo "4. Run './scripts/run-nodes.sh' to start nodes"