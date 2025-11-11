# Certus

Trustless deterministic compute via WebAssembly fraud proofs on Arbitrum.

## Problem

Centralized compute providers require trust. Existing decentralized solutions use committees (weak security) or ZK proofs (expensive, not general-purpose).

## Solution

Fraud proofs with economic security. Executors stake 2x collateral, verifiers independently re-execute jobs, disputes resolved by on-chain Wasm execution via Arbitrum Stylus. One honest verifier is enough.

## How It Works

1. Client submits code + input + USDC payment
2. Executor accepts job, stakes 200% collateral
3. Executor runs computation off-chain, signs output hash
4. Three verifiers independently re-execute
5. Match: executor paid, collateral returned
6. Mismatch: fraud proof triggers on-chain Wasm re-execution (Arbitrum Stylus)
7. Wrong party slashed, honest party compensated

**Security model:** Executor fraud costs \$200 collateral to attempt stealing \$100 job with 99%+ detection rate. Expected value: -\$197. Always economically irrational.

## Quick Start

### Try the Demo (No Blockchain Required)

```bash
# Prerequisites: Node.js 18+, Rust 1.70+
cd demo
npm install
npm run demo
```

Open http://localhost:3000

Write Python code, watch it compile to Wasm, execute on a remote node, and get verified by three independent nodes. Real cryptographic signatures, real fraud detection, no blockchain deployment needed.

### Build Production Nodes

```bash
# Prerequisites: Rust 1.75+, Foundry
cargo build --release
forge build
```

### Deploy Contracts

```bash
cd contracts
forge script script/Deploy.s.sol --rpc-url arbitrum-sepolia --broadcast
```

### Run Nodes

```bash
export ARBITRUM_RPC="https://sepolia-rollup.arbitrum.io/rpc"
export EXECUTOR_KEY="0x..."
export VERIFIER_KEY="0x..."
export ESCROW_ADDRESS="0x..."

./target/release/executor
./target/release/verifier
```

## Architecture

### Core Components

```
certus/
├── contracts/          # Solidity contracts (Arbitrum)
│   ├── CertusEscrow.sol       # Job escrow + fraud proofs
│   ├── CertusJobs.sol         # Job state machine
│   ├── CertusVerifier.sol     # VRF verifier selection
│   ├── CertusBisection.sol    # Interactive dispute narrowing
│   ├── CertusWasmProof.sol    # Stylus Wasm re-execution
│   ├── CertusInsurancePool.sol # Executor slashing insurance
│   └── CertusToken.sol        # CERTUS governance token
├── stylus-executor/    # Rust on-chain fraud proof verifier
│   ├── lib.rs          # Stylus contract (Rust to Wasm)
│   └── ARCHITECTURE.md # Design documentation
├── node/               # Rust execution nodes
│   ├── executor/       # Off-chain compute executor
│   ├── verifier/       # Independent fraud verifier
│   └── common/         # Shared types, crypto, contracts
├── python-verifier/    # Python → Wasm compiler (6k lines)
│   ├── compiler/       # AST → IR → Wasm pipeline
│   └── api/            # REST/WebSocket API
├── demo/               # Interactive demonstration
│   ├── coordinator/    # Job queue + WebSocket server
│   ├── executor/       # Demo executor node
│   ├── verifier/       # Demo verifier nodes
│   ├── frontend/       # Next.js + Monaco Editor
│   └── python-cli/     # Rust CLI wrapper
├── cli/                # TypeScript CLI tool
├── testvectors/        # Wasm test modules (.wat → .wasm)
└── scripts/            # Build, deploy, automation
```

### Tech Stack

- **Smart Contracts:** Solidity 0.8.24 (Foundry)
- **Fraud Proof Verifier:** Rust → Arbitrum Stylus (on-chain Wasm)
- **Execution Nodes:** Rust (Tokio, Wasmtime 15.0.1, Ethers)
- **Python Compiler:** Rust (rustpython-parser, wasm-encoder)
- **Demo:** Node.js (Express, WebSocket, SQLite)
- **Frontend:** Next.js, TypeScript, Monaco Editor
- **CLI:** TypeScript (Commander, Ethers v6)
- **Blockchain:** Arbitrum (Stylus for on-chain Wasm execution)

## Python Verifier

Certus includes a production-grade Python → Wasm compiler for verifiable computation.

### Supported Python

**Core language:**
- Integers (no floats)
- Strings, lists, dicts
- Functions (no closures yet)
- Control flow (if/while/for)
- Arithmetic, comparisons

**Deterministic stdlib:**
- `hashlib`: SHA256, SHA512, Blake2
- `json`: loads/dumps
- `base64`: encode/decode
- `collections`: OrderedDict, Counter
- `itertools`: combinations, permutations

**Prohibited (non-deterministic):**
- Floats (use fixed-point)
- Random, time, network, file I/O
- Dynamic code (eval/exec)
- Threading, async

### Example

```python
def solve():
    # Find 10,000th prime number
    primes = []
    n = 2
    while len(primes) < 10000:
        is_prime = True
        for p in primes:
            if p * p > n:
                break
            if n % p == 0:
                is_prime = False
                break
        if is_prime:
            primes.append(n)
        n += 1
    return {'result': primes[-1]}
```

Compiles to deterministic Wasm, executes off-chain, verifies on-chain if disputed.

## Economics

### Fee Structure (Tiered)

| Job Value | Protocol Fee | Example                 |
|-----------|--------------|-------------------------|
| $0-10 | 1.0%, min $0.10 | \$5 job = $0.10 (2%)    |
| $10-100 | 1.5%, min $0.50 | \$50 job = $0.75 (1.5%) |
| $100-1k | 2.0%, min $2.00 | \$500 job = $10 (2%)    |
| $1k-10k | 1.5%, min $20 | \$5k job = $75 (1.5%)   |
| $10k+ | 1.0%, min $150 | \$50k job = $500 (1%)   |

### Verifier Income

**Dynamic subsidy guarantees $200/month minimum:**

| Stage | Jobs/Month | Fee Income | CERTUS Emissions | Dynamic Subsidy | Total |
|-------|-----------|------------|------------------|----------------|-------|
| Cold start | 5 | $0.002 | $50 | $150 | $200 |
| Early growth | 100 | $0.015 | $20 | $180 | $200 |
| Scaling | 10,000 | $4.50 | $150 | $45.50 | $200 |
| Self-sustaining | 500,000 | $75 | $80 | $0 | $155+ |

Net income after \$50/month server costs: \$150/month = 1,800% APY on \$1,000 stake.

### Executor Economics

- Fixed 2.0x collateral (all executors equal, no reputation advantages)
- 100% slash + 30-day ban (first fraud)
- Permanent ban (second fraud)
- Optional: stake 50k CERTUS for insurance pool (covers 50% slashing)

### Attack Economics (All Negative EV)

**Executor fraud:**
- Cost: \$200 collateral (2.0x on $100 job)
- Gain: \$100 (if undetected)
- Detection rate: 99% (3 verifiers + client)
- Expected value: 1% × \$100 - 99% × \$200 = -\$197
- Verdict: Always economically irrational

**Sybil verifier attack:**
- Cost: 10 nodes × \$1,000 stake = \$10,000
- Selection rate: 5% (10 of 200 verifiers)
- Income: \$150/month net
- Opportunity cost: \$42/month (5% DeFi yield)
- Breakeven: 93 months (7.75 years)
- Verdict: Unprofitable

**Client griefing:**
- Cost: $5 bond (lost after timeout)
- Gain: $0 (executor claims timeout)
- Ban: After 3 offenses
- Verdict: Pure negative EV

## Security

### Determinism Guarantees

- Wasmtime 15.0.1 pinned (SHA256 verified on startup)
- No floats, no WASI, no threads, no SIMD
- Static analysis rejects non-deterministic modules at registration
- Multi-vector testing: 10 runs per test, 3+ platforms (Linux/Mac/Windows)
- Escape hatch: Bisection >50 rounds = refund both parties, 10% penalty

### Fraud Detection

- Challenge window: 1 hour (covers all timezones, prevents timezone attacks)
- Interactive bisection: Narrows disputes to 32 bytes (log₂ complexity, 20 rounds max)
- On-chain execution: Arbitrum Stylus Wasm re-execution
- Gas optimization: 3.8M gas (bisection) vs 12.5M (naive re-execution) = 97% savings
- Executor timeout: 5 minutes per bisection round or 100% slashed

### Network Resilience

- VRF verifier selection (weighted by CERTUS stake, max 2.0x boost)
- 3 primary + 3 backup verifiers (auto-replacement if offline)
- Heartbeat monitoring: Every 10 minutes (or ineligible)
- Network partition protection: >50% verifiers miss = grace period (no slashing)
- Geographic diversity: Max 30% from single region
- Verifier non-response: 30 minutes deadline or 50% slashed

### Data Availability

**Two-tier system:**
- Input ≤100KB: Stored in calldata (~$2 on Arbitrum), permanently available
- Input >100KB: Arweave TX ID required + 10% bond (slashed if data disappears)

## CERTUS Token

100M fixed supply, no inflation.

### Distribution

- 30% Treasury (bootstrap, insurance, grants)
- 25% Verifier rewards (48-month vest, backloaded curve)
- 20% Team (12-month cliff, 48-month vest)
- 15% DEX liquidity (locked)
- 10% Airdrop (early users)

### Seven Utilities

1. **Verifier boost:** Tiered selection weight (0x: 1.0x, 10k: 1.2x, 50k: 1.5x, 200k: 2.0x max)
2. **Fee discount:** Pay 100% in CERTUS for 40% discount
3. **Revenue share:** veCERTUS holders earn 50% of protocol fees in USDC
4. **Executor insurance:** Stake 50k CERTUS to join pool (covers 50% slashing losses)
5. **Priority execution:** Burn 100 CERTUS to skip queue
6. **Governance:** Vote on fees, treasury allocation, protocol parameters
7. **Buyback & burn:** 30% of fees used for market buys (deflationary)

## Development

### Build All Components

```bash
# Rust nodes + Python compiler
cargo build --release

# Solidity contracts
cd contracts && forge build

# Stylus fraud proof verifier
cd stylus-executor && cargo build --release --target wasm32-unknown-unknown

# TypeScript CLI
cd cli && npm install && npm run build

# Demo
cd demo && npm install
```

### Run Tests

```bash
# Rust tests
cargo test --all

# Contract tests
cd contracts && forge test

# Determinism tests (cross-platform)
./scripts/test-determinism.sh
```

### Docker Deployment

```bash
./scripts/docker-build.sh
docker-compose up -d
```

## Scripts

All scripts in `scripts/` directory:

- `setup-env.sh`: Install dependencies (Rust, Foundry, Node.js)
- `build-rust.sh`: Build all Rust components
- `deploy.sh`: Deploy contracts to Arbitrum testnet/mainnet
- `run-nodes.sh`: Start executor + verifier nodes
- `submit-job.sh`: Submit test job via CLI
- `test-determinism.sh`: Verify determinism across platforms
- `docker-build.sh`: Build Docker images

See `scripts/README.md` for detailed documentation.

## Use Cases

- **FFmpeg video transcoding:** H.264 encoding, 10GB videos
- **Hash grinding:** SHA-256 mining, cryptographic workloads
- **Monte Carlo simulation:** Scientific computation, risk analysis
- **ML inference:** Quantized models (≤24KB Wasm)
- **Smart contract analysis:** Rug pull detection, security auditing
- **Data processing:** ETL pipelines, batch transformations

## Documentation


### Component Documentation
- **[contracts/README.md](contracts/README.md)** - Solidity contracts, deployment, testing
- **[stylus-executor/README.md](stylus-executor/README.md)** - On-chain fraud proof verification
- **[stylus-executor/ARCHITECTURE.md](stylus-executor/ARCHITECTURE.md)** - Determinism design, security analysis
- **[demo/README.md](demo/README.md)** - Interactive demo setup, API reference, architecture
- **[python-verifier/README.md](python-verifier/README.md)** - Compiler architecture, language support, API
- **[scripts/README.md](scripts/README.md)** - Build automation, deployment scripts, utilities

### Additional Resources
- **[LICENSE](LICENSE)** - MIT License
- **[.gitignore](.gitignore)** - Version control exclusions

## Links

- **Website:** https://certuscompute.com
- **Live Demo:** https://certus.run
- **Whitepaper:** [Whitepaper.pdf](Whitepaper.pdf)

## License

MIT License. See [LICENSE](LICENSE) for details.

---

Built for trustless general-purpose computation. No oracles, no committees, no trust assumptions.
