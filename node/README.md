# Certus Node

Rust implementation of executor and verifier nodes.

## Architecture

```
common/         # Shared types and crypto
executor/       # Runs WebAssembly jobs
verifier/       # Verifies execution results
```

## Build

```bash
cargo build --release
```

## Run

```bash
# Executor
./target/release/executor <rpc_url> <private_key> <contract_address>

# Verifier
./target/release/verifier <rpc_url> <private_key> <contract_address>
```

## Configuration

- `rpc_url`: Arbitrum RPC endpoint
- `private_key`: Ethereum private key (with USDC for collateral)
- `contract_address`: Deployed CertusEscrow contract

## Docker

```bash
docker build -t certus-node .
docker run certus-node executor <args>
```