# VerifiedPump - Unruggable Token Launchpad

One-click token launchpad with built-in rug pull protection powered by Certus.

## Features

- Certus verification (proves code has no rug vectors)
- Automatic Uniswap V2 liquidity addition
- Built-in LP lock (minimum 6 months)
- Ownership renouncement enforced
- Verification NFT badge
- Anti-sniper protection (5 minute period)

## Architecture

### VerifiedPumpFactory.sol
Main factory contract handling:
1. Submit token bytecode for verification
2. Certus runs static analyzer off-chain
3. Deploy if clean, revert if rug vectors detected
4. Add liquidity and lock LP tokens
5. Mint verification NFT to creator

### TokenTemplates.sol
Example templates for verified tokens:
- Standard ERC20
- Deflationary token
- Reflection token
- All templates pre-verified to be unruggable

## Deployment

```bash
forge script script/DeployVerifiedPump.s.sol \
  --rpc-url $RPC_URL \
  --broadcast \
  --verify
```

## Economics

- Launch fee: 0.5% of liquidity
- Minimum liquidity: 0.05 ETH
- LP lock minimum: 180 days
- Verification cost: 0.002 ETH
