# Solidity Rug Pull Analyzer

Static analyzer that detects rug pull vectors in Solidity token contracts.

## Detection Coverage

- Owner mint functions (severity 10)
- Pausable mechanisms (severity 9)
- Blacklist functions (severity 8)
- Owner withdrawal backdoors (severity 9)
- Upgradeable proxies (severity 10)
- Variable fees (severity 7)
- Max transaction limits (severity 5)
- Transfer cooldowns (severity 6)

## Building

```bash
cargo build --release --target wasm32-unknown-unknown
```

Output: `target/wasm32-unknown-unknown/release/analyzer.wasm`

## Usage

Input: Solidity source code (UTF-8 encoded)

Output:
- `CLEAN` if no rug vectors detected
- `RUG:<json>` with severity scores and locations

Example:
```json
{
  "is_safe": false,
  "score": 20,
  "rug_vectors": [{
    "severity": 10,
    "category": "OWNER_MINT",
    "description": "Owner can mint unlimited tokens",
    "location": "function mint"
  }]
}
```

## Integration

Used by VerifiedPump to verify token contracts before deployment. Runs deterministically in Certus to prevent disputes.
