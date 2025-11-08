# Certus Demo

Demonstration of trustless compute verification (without blockchain deployment).

## Overview

Demonstrates Python code compiled to deterministic Wasm, executed by a remote node, and independently verified by multiple nodes using cryptographic signatures.

**What's Real:**
- Python to Wasm compilation (deterministic, reproducible)
- Ed25519 cryptographic signatures
- Multi-verifier consensus (3 independent nodes)
- Fraud detection (hash mismatches caught immediately)
- Real-time WebSocket updates

**What's Centralized (Demo Only):**
- Executor/verifier selection (will use VRF on-chain)
- Payment simulation

## Prerequisites

- Node.js 18+ ([download](https://nodejs.org))
- Rust 1.70+ ([download](https://rustup.rs))

## Setup

**Windows:**
```powershell
.\scripts\setup.ps1
```

**Linux/Mac:**
```bash
chmod +x scripts/setup.sh
./scripts/setup.sh
```

**Manual:**
```bash
npm install
cd frontend && npm install && cd ..
cd ../python-verifier && cargo build --release && cd ../demo
cd python-cli && cargo build --release && cd ..
```

## Launch

```bash
npm run demo
```

Open http://localhost:3000

## Architecture

```
Frontend (Next.js + Monaco Editor)
    |
    | WebSocket + REST API
    |
Coordinator (Express + SQLite + WebSocket)
    |
    +--- Executor (compiles Python -> Wasm, executes, signs)
    |
    +--- Verifier 1 (re-executes, signs, compares)
    +--- Verifier 2
    +--- Verifier 3
```

## API

**Submit Job:**
```bash
POST http://localhost:4000/api/jobs
Content-Type: application/json

{"pythonCode": "def solve():\n    return {'result': 42}"}
```

**Get Job Status:**
```bash
GET http://localhost:4000/api/jobs/:jobId
```

**Network Stats:**
```bash
GET http://localhost:4000/api/stats
```

**WebSocket:**
```javascript
const ws = new WebSocket('ws://localhost:4000');
ws.send(JSON.stringify({ type: 'register', nodeType: 'frontend' }));
```

## Project Structure

```
demo/
├── coordinator/     # Job queue + WebSocket server (Express + SQLite)
├── executor/        # Wasm compilation and execution
├── verifier/        # Independent re-execution nodes
├── frontend/        # Next.js + Monaco Editor
├── python-cli/      # Rust CLI wrapper around python-verifier
├── shared/          # Crypto utilities (Ed25519, SHA-256)
├── examples/        # Challenge templates
└── scripts/         # Setup and launch automation
```

## Manual Control

```bash
# Terminal 1
npm run coordinator

# Terminal 2
npm run executor

# Terminal 3-5
VERIFIER_ID=verifier-nyc npm run verifier
VERIFIER_ID=verifier-berlin npm run verifier
VERIFIER_ID=verifier-tokyo npm run verifier

# Terminal 6
npm run frontend
```

## Troubleshooting

**Port 4000 in use:**
```bash
# Windows
netstat -ano | findstr :4000
taskkill /PID <PID> /F

# Linux/Mac
lsof -ti:4000 | xargs kill -9
```

**Rust build fails:**
```bash
cd ../python-verifier
cargo build --release
cd ../demo
```

**Verifiers not connecting:**
Restart the demo. Check logs for "Registered with coordinator".

## Technical Details

### Data Flow

1. User submits Python code via frontend
2. Coordinator generates jobId = SHA256(code + nonce)
3. Executor compiles Python to Wasm, calculates wasmHash
4. Executor executes Wasm, signs outputHash with Ed25519
5. Coordinator selects 3 verifiers
6. Verifiers re-execute independently, sign their outputHash
7. Coordinator compares hashes: 3/3 match = verified, mismatch = fraud

### State Machine

```
queued -> compiling -> executing -> verifying -> verified
                                              \-> fraud
```

### Cryptographic Proof

- Code hash: SHA-256(pythonCode)
- Wasm hash: SHA-256(wasmBytes)
- Output hash: SHA-256(output)
- Executor signature: Ed25519(outputHash, executorPrivKey)
- Verifier signatures: Ed25519(outputHash, verifierPrivKey) x3

Total proof size: 320 bytes (32 + 32 + 256)


## License

See ../LICENSE
