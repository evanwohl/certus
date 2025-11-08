#!/bin/bash

# Certus Demo Setup Script

set -e

echo "              CERTUS DEMO SETUP                        "
echo ""

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v node &> /dev/null; then
    echo "Node.js not found. Please install Node.js 18+"
    exit 1
fi
echo "Node.js $(node --version)"

if ! command -v npm &> /dev/null; then
    echo "npm not found. Please install npm"
    exit 1
fi
echo "npm $(npm --version)"

if ! command -v cargo &> /dev/null; then
    echo "Cargo not found. Please install Rust: https://rustup.rs"
    exit 1
fi
echo "Cargo $(cargo --version)"

echo ""
echo "Installing Node.js dependencies..."
npm install

echo ""
echo "Installing frontend dependencies..."
cd frontend
npm install
cd ..

echo ""
echo "Building python-verifier library..."
cd ../python-verifier
cargo build --release
cd ../demo

echo ""
echo "Building python-cli..."
cd python-cli
cargo build --release
cd ..

echo ""
echo "Setup complete!"
echo ""
echo "Launch the demo:"
echo "   npm run demo"
echo ""
