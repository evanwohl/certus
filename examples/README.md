# Certus Examples

Production examples demonstrating Certus protocol usage.

## Examples

### analyzer
Static analyzer for Solidity contracts. Detects rug pull vectors (mint functions, owner withdrawals, pausable mechanisms, blacklists). Compiles to Wasm for deterministic on-chain verification.

### verifiedpump
Unruggable token launchpad. Users submit token bytecode, Certus verifies it's clean using the analyzer, then deploys with automatic liquidity lock. Demonstrates end-to-end verification workflow.

## Building

```bash
# Build analyzer
cd analyzer
cargo build --release --target wasm32-unknown-unknown

# Deploy verifiedpump
cd verifiedpump
forge build
forge script script/DeployVerifiedPump.s.sol --broadcast
```

## Use Cases

- Smart contract verification (prevents rug pulls)
- Deterministic static analysis
- Trustless code auditing
- Fair token launches
