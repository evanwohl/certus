# Certus Stylus Executor

On-chain deterministic WebAssembly execution for fraud proof verification.

## Overview

This contract deploys to Arbitrum as a Stylus program (Rust compiled to Wasm) and provides cryptographically identical execution to off-chain executor nodes. When a verifier detects fraud, this contract re-executes the disputed job on-chain to determine ground truth.

## Architecture

```
Off-chain:                                      On-chain (Stylus):
node/executor/sandbox.rs  <- IDENTICAL CONFIG -> stylus-executor/lib.rs
  |                                               |
  Wasmtime 15.0.1                                 Wasmtime 15.0.1
  Deterministic config                            Deterministic config
  No floats/WASI/threads                          No floats/WASI/threads
  Fuel + memory limits                            Fuel + memory limits
```

## Determinism Guarantees

1. Wasmtime 15.0.1 pinned (same version as off-chain)
2. Identical configuration to node/executor/src/sandbox.rs
3. No floating point operations (0x43-0xBF opcodes rejected)
4. No WASI imports (wasi_snapshot string rejected)
5. No thread operations (0xFE prefix rejected)
6. Fuel and memory limits enforced identically
7. Single-threaded execution only
8. NaN canonicalization enabled
9. Static memory bounds (64MB max)

## Security Properties

Critical invariants:
- Module validation matches sandbox.rs exactly
- Execution output identical to off-chain for valid jobs
- Fraud detection rate: ~100% (deterministic re-execution)
- Gas cost: ~3.8M for bisection
- No trust assumptions (cryptographic determinism)

## Deployment

### Prerequisites

```bash
cargo install cargo-stylus
```

### Deploy to Arbitrum Sepolia

```bash
cd stylus-executor
cargo stylus deploy --private-key $PRIVATE_KEY --endpoint https://sepolia-rollup.arbitrum.io/rpc
```

### Deploy to Arbitrum Mainnet

```bash
cargo stylus deploy --private-key $PRIVATE_KEY --endpoint https://arb1.arbitrum.io/rpc
```

### Verify Deployment

```bash
cargo stylus verify --deployment-tx $TX_HASH
```

## Testing

### Unit Tests

```bash
cargo test
```

### Integration Test (with off-chain node)

```bash
# Start local executor
cd ../node/executor
cargo run -- <rpc> <key> <contract>

# Submit test job
cd ../../stylus-executor
cargo test --test integration -- --ignored
```

## Contract Interface

```solidity
interface ICertusStylusExecutor {
    /// Execute Wasm module with deterministic guarantees
    function execute(
        bytes calldata wasm,
        bytes calldata input,
        uint256 fuelLimit,
        uint256 memLimit
    ) external returns (bytes memory output);

    /// Get total execution count
    function getExecutionCount() external view returns (uint256);

    /// Verify previous execution result
    function getExecutionResult(bytes32 executionId) external view returns (bytes32);
}
```

## Error Codes

| Code | Error |
|------|-------|
| 0xFF01 | ModuleTooLarge (>24KB) |
| 0xFF02 | InvalidWasmMagic |
| 0xFF03 | InvalidWasmVersion |
| 0xFF04 | FloatOpcodeDetected |
| 0xFF05 | WasiImportDetected |
| 0xFF06 | ThreadOpcodeDetected |
| 0xFF07 | CompilationFailed |
| 0xFF08 | InstantiationFailed |
| 0xFF09 | ExecutionFailed |
| 0xFF0A | InvalidFuelLimit |
| 0xFF0B | InvalidMemoryLimit |
| 0xFF0C | OutOfFuel |
| 0xFF0D | OutOfMemory |


## Integration with CertusEscrow

The escrow contract calls this executor during fraud proofs:

```solidity
// In CertusEscrow.sol
function fraudOnChain(
    bytes32 jobId,
    bytes calldata wasm,
    bytes calldata input,
    bytes calldata claimedOutput
) external {
    // Re-execute on-chain
    bytes memory actualOutput = IStylusExecutor(stylusExecutor).execute(
        wasm,
        input,
        job.fuelLimit,
        job.memLimit
    );

    // Compare hashes
    bytes32 claimedHash = keccak256(claimedOutput);
    bytes32 actualHash = keccak256(actualOutput);

    if (claimedHash != actualHash) {
        // Fraud confirmed: slash executor 100%
        _slashExecutor(jobId);
    }
}
```

## Development

### Build

```bash
cargo build --release
```

### Export ABI

```bash
cargo stylus export-abi
```

### Check Size

```bash
cargo stylus check --wasm-file target/wasm32-unknown-unknown/release/certus_stylus_executor.wasm
```


## License

MIT
