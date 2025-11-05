# Certus Scripts

Automation scripts for building, deploying, and running Certus nodes.

## Prerequisites

- Rust 1.75+ (https://rustup.rs)
- Foundry (https://book.getfoundry.sh)
- Docker (optional, for containerized deployment)

## Available Scripts

### setup-env.sh
Initial environment setup. Checks prerequisites, installs dependencies, creates .env file.
```bash
./scripts/setup-env.sh
```

### build-rust.sh
Builds Rust nodes (executor and verifier) in release mode with optimizations.
```bash
./scripts/build-rust.sh
```

### deploy.sh
Deploys CertusEscrow contract to Arbitrum (testnet or mainnet).
```bash
./scripts/deploy.sh [testnet|mainnet]
```
Requires `.env.testnet` or `.env.mainnet` with:
- `RPC_URL`
- `DEPLOYER_PRIVATE_KEY`
- `ETHERSCAN_API_KEY` (optional, for verification)

### run-nodes.sh
Starts executor and verifier nodes.
```bash
export RPC_URL="https://sepolia-rollup.arbitrum.io/rpc"
export EXECUTOR_KEY="your_executor_private_key"
export VERIFIER_KEY="your_verifier_private_key"
export CONTRACT_ADDRESS="deployed_contract_address"
./scripts/run-nodes.sh
```

### submit-job.sh
Submits a test job to the deployed contract.
```bash
export CONTRACT_ADDRESS="0x..."
export RPC_URL="https://..."
export CLIENT_KEY="your_client_private_key"
./scripts/submit-job.sh [wasm_file] [input_file] [payment_usdc]
```
Defaults:
- Wasm: `testvectors/sha256.wasm`
- Input: `testvectors/input1.bin`
- Payment: 50 USDC

### test-determinism.sh
Tests WebAssembly execution determinism.
```bash
./scripts/test-determinism.sh
```
Runs multiple iterations to ensure identical outputs.

### docker-build.sh
Builds Docker images for containerized deployment.
```bash
./scripts/docker-build.sh
```
Optional: Set `REGISTRY` to tag for remote registry.

## Typical Workflow

1. **Initial Setup**
   ```bash
   ./scripts/setup-env.sh
   ```

2. **Build Nodes**
   ```bash
   ./scripts/build-rust.sh
   ```

3. **Deploy Contracts**
   ```bash
   ./scripts/deploy.sh testnet
   ```

4. **Start Nodes**
   ```bash
   source deployed-addresses.testnet.env
   ./scripts/run-nodes.sh
   ```

5. **Submit Test Job**
   ```bash
   ./scripts/submit-job.sh
   ```

## Docker Deployment

```bash
# Build images
./scripts/docker-build.sh

# Run with docker-compose
docker-compose up -d

# View logs
docker-compose logs -f executor
docker-compose logs -f verifier-1
```

## Environment Variables

Create `.env` with:
```bash
# Network
RPC_URL=https://sepolia-rollup.arbitrum.io/rpc
CONTRACT_ADDRESS=0x...

# Keys (without 0x prefix)
EXECUTOR_KEY=...
VERIFIER_KEY_1=...
VERIFIER_KEY_2=...
VERIFIER_KEY_3=...
CLIENT_KEY=...

# Optional
ETHERSCAN_API_KEY=...
REGISTRY=your-docker-registry.com
```

## Notes

- Scripts use `set -e` for fail-fast behavior
- All builds use `--release` for optimized binaries
- Contract deployment saves addresses to `deployed-addresses.{network}.env`
- Nodes require funded wallets for gas and collateral