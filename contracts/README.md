# Certus Contracts

Solidity smart contracts for the Certus protocol, deployed on Arbitrum.

## Contracts

- **CertusEscrow.sol** - Main escrow contract handling job creation, execution, and fraud proofs
- **CertusToken.sol** - CERTUS governance token with 7 utility mechanisms

## Structure

```
contracts/
├── src/               # Contract source files
├── test/              # Foundry tests
├── script/            # Deployment scripts
└── foundry.toml       # Foundry configuration
```

## Development

### Build
```bash
forge build
```

### Test
```bash
forge test
```

### Deploy
```bash
forge script script/Deploy.s.sol --rpc-url $RPC_URL --broadcast
```

## Key Features

- **Deterministic compute verification** via Arbitrum Stylus on-chain Wasm execution
- **Fixed 2.0x collateral** for all executors (fair competition)
- **Tiered fee structure** ($0.10 to $500 based on job value)
- **Dynamic verifier subsidies** (guarantees $200/month minimum income)
- **Fraud proofs** with 100% collateral slashing
- **Challenge window** of 1 hour (timezone attack impossible)
