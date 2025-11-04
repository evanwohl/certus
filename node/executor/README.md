# Certus Executor

Executor node that accepts and runs compute jobs.

## Configuration

Create `config.yml`:

```yaml
rpcUrl: "https://sepolia-rollup.arbitrum.io/rpc"
privateKey: "0x..."
contractAddress: "0x..."
certusTokenAddress: "0x..."
maxCollateralCapital: "10000000000000000000" # 10 ETH
minCollateralRatio: 100  # 1.0x
maxCollateralRatio: 300  # 3.0x
```

## Running

```bash
../gradlew run
```

## Capital Efficiency

Stake CERTUS tokens to reduce collateral requirements:
- 10k CERTUS: 0.8x collateral
- 50k CERTUS: 0.6x collateral
- Insurance pool: 50% slashing coverage

## Security

- Deterministic Wasm execution (Wasmtime 15.0.1)
- No float operations
- No WASI imports
- Memory isolation per job
- Ed25519 signature verification
