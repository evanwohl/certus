# Certus Scripts

Automation and deployment scripts for the Certus protocol.

## Setup Scripts

### initialize.sh / initialize.bat
Complete setup including dependency checks, builds, configs, and verification.
Recommended for first-time setup.

### setup.sh / setup.bat
Quick setup for building protocol components and generating configs.

### verify-setup.sh / verify-setup.bat
Validates that all components are correctly installed and configured.

## Development Scripts

### build-all.sh
Builds all components (contracts, nodes, CLI) in correct order.

### test-all.sh
Runs full test suite including:
- Solidity contract tests
- Java unit tests
- Determinism tests
- Integration tests

### deploy-local.sh
Deploys contracts to local Hardhat/Anvil node for development.

## CI/CD

Scripts are designed to be run both locally and in CI environments. Exit codes indicate success (0) or failure (non-zero).
