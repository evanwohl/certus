# Certus Verifier

Verifier node that validates executor results and submits fraud proofs.

## Configuration

Create `config.yml`:

```yaml
rpcUrl: "https://sepolia-rollup.arbitrum.io/rpc"
privateKey: "0x..."
contractAddress: "0x..."
heartbeatIntervalSeconds: 600  # 10 minutes
stake: "1000000000000000000000"  # 1000 USDC
```

## Running

```bash
../gradlew run
```

## Economics

- Required stake: $1,000 USDC minimum
- Monthly income: $200 (fees + subsidies + CERTUS emissions)
- Heartbeat required every 10 minutes
- 50% slash for missed responses
- 20% bounty for successful fraud proofs

## Selection Weight

Stake CERTUS tokens for higher selection probability:
- 0 CERTUS: 1.0x weight
- 10k CERTUS: 1.2x weight
- 50k CERTUS: 1.5x weight
- 200k CERTUS: 2.0x weight (maximum)

## Responsibilities

1. Maintain heartbeat every 10 minutes
2. Re-execute assigned jobs within 30 minutes
3. Submit fraud proof if output mismatch detected
4. Participate in bisection protocol if challenged
