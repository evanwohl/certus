# Certus

Trustless deterministic compute via WebAssembly fraud proofs.

**Problem:** Can't trust centralized compute providers. Existing solutions use committees (weak security) or expensive ZK proofs (impractical for general compute).

**Solution:** Fraud proofs with real economic stakes. Executors post 2x collateral, verifiers earn fees to check work, disputes resolved by re-running Wasm on-chain.

## Architecture

**Three actors:**
- **Clients** - Submit jobs with USDC payment
- **Executors** - Run Wasm modules, post 2x collateral, get paid if honest
- **Verifiers** - Re-check results, earn fees, submit fraud proofs if needed

**How it works:**
1. Client uploads Wasm + input, deposits payment
2. Executor accepts job, posts 200% collateral (gets slashed if fraud)
3. Executor runs job off-chain, signs output hash
4. Three verifiers re-run the job independently
5. If match: executor paid, collateral returned
6. If mismatch: fraud proof triggers on-chain Wasm execution (Arbitrum Stylus)
7. Wrong party gets slashed, honest party keeps collateral

**Security model:** 1 honest verifier is enough. 99% fraud detection means losing $200 collateral to steal $100 job. Always negative EV.

## Build

Prerequisites: Rust 1.75+, Foundry

```bash
# Setup environment
./scripts/setup-env.sh

# Build Rust nodes
./scripts/build-rust.sh

# Run tests
cargo test --all
```

## Deploy Contracts

```bash
# Deploy to Arbitrum testnet
./scripts/deploy.sh testnet

# Or deploy to mainnet
./scripts/deploy.sh mainnet
```

Requires `.env.testnet` or `.env.mainnet` with your keys.

## Run Nodes

```bash
# Set environment variables
export RPC_URL="https://sepolia-rollup.arbitrum.io/rpc"
export EXECUTOR_KEY="your_executor_private_key"
export VERIFIER_KEY="your_verifier_private_key"
export CONTRACT_ADDRESS="deployed_contract_address"

# Start both nodes
./scripts/run-nodes.sh

# Or run individually:
./target/release/executor $RPC_URL $EXECUTOR_KEY $CONTRACT_ADDRESS
./target/release/verifier $RPC_URL $VERIFIER_KEY $CONTRACT_ADDRESS
```

## Submit Test Job

```bash
# Submit a job with test vectors
./scripts/submit-job.sh testvectors/sha256.wasm testvectors/input1.bin 50000000
```

Expected output:
```
Job created: 0xabcd1234...
Executor accepted job
Output received: 0x9f86d081...
Verifiers confirmed (3/3)
Job finalized in 18s
```

## Project Structure

```
contracts/       # Solidity contracts (Arbitrum)
node/            # Rust implementation
  executor/      # Compute executor
  verifier/      # Fraud verifier
  common/        # Shared types and crypto
testvectors/     # WebAssembly test modules
```

## Economics

**Fee structure:**
- $0-10 jobs: 1.0% fee
- $10-100 jobs: 1.5% fee
- $100-1k jobs: 2.0% fee
- $1k+ jobs: 1.5% fee

**Verifier income:**
- Base fees from jobs
- CERTUS token emissions (vesting over 48 months)
- Dynamic subsidy guarantees $200/month minimum during bootstrap
- Stake CERTUS tokens for higher selection weight (max 2x)

**Executor requirements:**
- Fixed 2x collateral on all jobs
- 100% slash + 30-day ban on first fraud
- Permanent ban on second fraud
- Optional: stake 50k CERTUS for insurance pool (covers 50% of slashing)

**Attack economics:**
- Executor fraud: -$197 expected value (99% catch rate, $200 collateral at risk)
- Sybil verifier attack: 93-month breakeven (not worth it)
- Client griefing: loses $5 bond after 3 offenses

## Security

**Determinism enforcement:**
- Wasmtime 15.0.1 pinned (SHA256 verified on startup)
- No floats, no WASI, no threads, no SIMD
- Static analysis rejects non-deterministic modules
- Multi-vector testing (10 runs per test case, 3+ platforms)

**Fraud detection:**
- 1-hour challenge window (covers all timezones)
- Interactive bisection narrows disputes to 32 bytes (gas optimization)
- On-chain Wasm execution via Arbitrum Stylus
- Verifiers must heartbeat every 10 minutes or get slashed

**Network resilience:**
- Grace period if >50% verifiers miss (prevents false slashing during outages)
- 3 backup verifiers auto-replace offline primaries
- Geographic diversity limit (max 30% from single region)

## Token Utilities

CERTUS token has 7 utilities:
1. Verifier selection weight (stake 200k for 2x boost)
2. Fee discount (pay in CERTUS for 40% off)
3. Revenue share (veCERTUS holders get 50% of protocol fees)
4. Executor insurance (stake 50k to join pool)
5. Priority queue (burn 100 CERTUS to skip line)
6. Governance (vote on fees, parameters)
7. Buyback & burn (30% of fees used for market buys)

100M fixed supply, no inflation.

## Examples

**VerifiedPump** - Unruggable token launchpad. Submit token bytecode, Certus verifies it has no rug vectors (mint functions, owner withdrawals, pausable), deploys if clean. Automatic liquidity lock for 6 months minimum.

**Static Analyzer** - Rust-based Solidity analyzer that detects 10 rug pull patterns. Compiles to Wasm for deterministic verification. Used by VerifiedPump but reusable for any contract auditing.

## Roadmap

**Phase 1 (Current):** MVP on Arbitrum testnet
- Core escrow contracts
- Executor + verifier nodes
- Fraud proof mechanism
- Example workloads (sha256, mandelbrot)

**Phase 2:** Mainnet launch
- External audit (Quantstamp/Trail of Bits)
- Mainnet deployment
- VerifiedPump marketing demo
- 20+ verifiers at launch

**Phase 3:** Scale
- Chainlink VRF for verifier selection
- Chainlink price feeds for CERTUS/USD conversion
- Multi-chain expansion (Optimism, Base)
- Enterprise partnerships

## Development

Run all tests:
```bash
cargo test --all
```

Test determinism:
```bash
./scripts/test-determinism.sh
```

Build Docker images:
```bash
./scripts/docker-build.sh
docker-compose up -d
```


## License

MIT License

---
