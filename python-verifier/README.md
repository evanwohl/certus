# Certus Python Verifier

Deterministic Python → Wasm compiler for verifiable compute on Certus protocol.

## What This Does

Compile Python scripts to deterministic Wasm, execute off-chain, prove correctness on-chain via Arbitrum Stylus re-execution. No oracles, no committees, no trust assumptions.

## How It Fits Into Certus

Certus is a **trustless general-purpose compute protocol**. This Python verifier is one reference implementation showing how to compile a high-level language to deterministic Wasm that's verifiable through Certus contracts.

```
┌─────────────────────────────────────────┐
│ Python Code (user submits)              │
└───────────────┬─────────────────────────┘
                ↓
┌─────────────────────────────────────────┐
│ Python Verifier                         │
│ - Compiles Python → deterministic Wasm  │
│ - Executes Wasm with Wasmtime 15.0.1    │
│ - Submits receipts to contracts         │
└───────────────┬─────────────────────────┘
                ↓
┌─────────────────────────────────────────┐
│ Certus Contracts (Arbitrum)             │
│ - CertusJobs: Job + payment escrow      │
│ - CertusVerifier: VRF verifier selection│
│ - CertusEscrow: Fraud proofs via Stylus │
└─────────────────────────────────────────┘
```

**Execution Flow:**
1. **Client** submits Python code + input + USDC payment
2. **Python Verifier** compiles to Wasm (cached by SHA256)
3. **Executor** accepts job, stakes 2x collateral, runs Wasm off-chain
4. **Executor** submits output hash + Ed25519 signature
5. **VRF** selects 3 verifiers (weighted by CERTUS stake, max 2x)
6. **Verifiers** re-execute Wasm, submit fraud proof if mismatch
7. **Arbitrum Stylus** re-executes Wasm on-chain if fraud claimed
8. **Result**: Executor slashed 100% (fraud) OR paid (correct)

## Compiler Architecture

**3-Stage Pipeline:**

```
Python Source Code
    ↓
rustpython-parser (official Python grammar)
    ↓
Python AST (validated for determinism)
    ↓
IR Lowering (expression → intermediate representation)
    ↓
Deterministic IR (lexically sorted locals, BTreeMap ordering)
    ↓
Wasm Codegen (per-IR gas metering, bump allocator)
    ↓
Wasm Module (SHA256 cached, 100% reproducible)
```

**Key Guarantees:**
- **Deterministic**: Same Python → same Wasm bytecode (always)
- **Reproducible**: Run on Linux/Mac/Windows → identical output
- **Auditable**: IR layer makes compilation transparent
- **Extensible**: Clean separation (AST → IR → Wasm)

## Supported Python

**Core Language:**
- Integers (no floats)
- Strings, lists, dicts
- Functions (no closures yet)
- Control flow (if/while/for)
- Arithmetic, comparisons

**Standard Library (Deterministic Subset):**
- `hashlib`: SHA256, SHA512, Blake2
- `json`: loads/dumps
- `base64`: encode/decode
- `collections`: OrderedDict, Counter
- `itertools`: combinations, permutations
- `math`: integer ops only (no sqrt/pow/floor)

**Prohibited (Non-Deterministic):**
- Floats (use fixed-point integers)
- Random, time, network, file I/O
- Dynamic code (eval/exec)
- Threading, async

## API

### Compile + Submit Job
```bash
POST /api/submit
{
  "python_code": "def fib(n):\n  return n if n <= 1 else fib(n-1) + fib(n-2)\n\nresult = fib(input['n'])",
  "input": {"n": 20},
  "payment_amount": "10000000",  # $10 USDC (6 decimals)
  "pay_token": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"  # USDC on Arbitrum
}
```

### Execute Job (Executor)
```bash
POST /api/execute/{job_id}
```

### Verify Job (Verifier)
```bash
POST /api/verify/{job_id}
```


## Security

**Determinism Guarantees:**
1. No floats (rejected at AST validation)
2. No time/random/network (import validation)
3. Fixed hash seeds (BTreeMap for locals + constants)
4. Deterministic locals ordering (lexical sort)
5. Deterministic gas cost (per-IR-node metering)
6. Deterministic memory layout (bump allocator)
7. Reproducible bytecode (SHA256 cached compilation)

**Attack Economics (All Negative EV):**
- Executor fraud: -$197 expected loss (99% catch rate, 2x collateral)
- Sybil verifiers: 93-month payback period (unprofitable)
- Client grief: -$5 bond, ban after 3 offenses
- Timezone attack: IMPOSSIBLE (1-hour challenge window)

**Network Protection:**
- VRF verifier selection (prevents targeting)
- 3 primary + 3 backup verifiers (redundancy)
- \>50% offline = grace period (no slashing)
- 10-minute heartbeat monitoring
- Geographic diversity (max 30% per region)

## Deployment

**Build:**
```bash
cd python-verifier
cargo build --release
```

**Run:**
```bash
export ARBITRUM_RPC=https://arb-mainnet.g.alchemy.com/v2/YOUR_KEY
export PRIVATE_KEY=0xYOUR_PRIVATE_KEY
export ESCROW_ADDRESS=0xYOUR_ESCROW_CONTRACT
export JOBS_ADDRESS=0xYOUR_JOBS_CONTRACT

./target/release/python-verifier \
  --rpc $ARBITRUM_RPC \
  --private-key $PRIVATE_KEY \
  --escrow $ESCROW_ADDRESS \
  --jobs $JOBS_ADDRESS \
  --port 8080
```

**Test:**
```bash
cargo test
```

## Performance

- **Compilation**: ~10ms per script (cached by SHA256)
- **Execution**: Wasmtime 15.0.1 (deterministic config)
- **Gas cost**: ~380k gas on-chain verification (Stylus)
- **Max script**: 24KB (on-chain storage limit)
- **Max input**: 100KB on-chain, >100KB needs Arweave + 10% bond
- **Bisection**: 3.8M gas worst-case (vs 12.5M naive)

## Fraud Proof Flow

1. Executor submits wrong outputHash
2. Verifier detects mismatch during re-execution
3. Verifier calls `fraudCommit(jobId, commitHash)` (hides outputHash)
4. Wait 2 minutes (prevents MEV)
5. Verifier calls `fraudReveal(jobId, wasmBytes, inputBytes, correctOutputHash)`
6. Stylus re-executes Wasm on-chain
7. Mismatch confirmed → executor slashed 100%
8. Verifier receives 20% bounty, client refunded 80%

**Bisection Optimization (Large Outputs):**
- Interactive narrowing: 20 rounds max (log₂ complexity)
- Executor timeout: 5 min per round (or slashed)
- Final dispute: 32 bytes executed on-chain
- Gas savings: 97% (3.8M vs 12.5M)

## Language Agnosticism

**Certus supports ANY language → Wasm:**
- Python (this repo)
- Rust, C, C++, Go, AssemblyScript
- Zig, Kotlin, Swift (via wasm targets)

**Requirements:**
1. Compile to deterministic Wasm
2. No floats, no WASI, no threads
3. Export `execute(input_ptr, input_len) -> output_len`
4. Memory import from `env.memory`
5. Gas metering hooks (automatic)

**Example (Rust):**
```rust
#[no_mangle]
pub extern "C" fn execute(input_ptr: i32, input_len: i32) -> i32 {
    // Read input from memory
    // Compute deterministically
    // Write output to memory
    // Return output length
}
```

## Why Python Specifically?

Python is a **reference implementation** demonstrating:
1. High-level language compilation to deterministic Wasm
2. AST → IR → Wasm pipeline architecture
3. Integration with Certus contracts
4. VRF verifier coordination
5. Economic security (2x collateral, fraud proofs)

**Other languages use the same contracts** (CertusJobs, CertusEscrow, CertusVerifier). No protocol changes needed.

## Contributing

This is a reference implementation. Contributions welcome for:
- Additional Python stdlib support
- Optimization passes in IR layer
- More comprehensive test vectors
- Language-specific verifiers (Go, Rust, etc.)

## License

MIT
