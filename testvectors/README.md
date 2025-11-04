# Certus Test Vectors

This directory contains deterministic test vectors for validating Wasm execution.

## Structure

```
/testvectors
  /wasm
    echo.wasm       - Echo input to output
    sha256.wasm     - SHA256 hash of input
    add.wasm        - Add two integers (simple arithmetic)
    mandelbrot.wasm - Generate Mandelbrot set (pure compute)
  /inputs
    input1.bin      - Test input (32 bytes)
    input2.bin      - Test input (1024 bytes)
  /outputs
    output1.bin     - Expected output for echo.wasm + input1.bin
    output2.bin     - Expected output for sha256.wasm + input1.bin
```

## Determinism Requirements

All test vectors MUST produce identical outputs across:
- Linux x86_64
- macOS ARM64 (M1/M2)
- macOS x86_64 (Intel)
- Windows x86_64
- CI environment (Ubuntu 22.04)

## Validation

Run the determinism harness:

```bash
./gradlew :executor:test --tests "net.certus.test.DeterminismTest"
```

This will execute each test vector N=10 times and verify output hashes match exactly.

## Creating New Test Vectors

1. Write a deterministic Wasm module (no floats, no syscalls, no randomness)
2. Compile with `wat2wasm` or Rust/AssemblyScript toolchain
3. Generate input binary
4. Run executor to produce output
5. Validate on all platforms
6. Add to this directory with documentation
