# Certus CLI

Command-line interface for interacting with the Certus protocol.

## Installation

```bash
npm install -g @certus/cli
```

## Usage

```bash
# Submit a compute job
certus submit-job \
  --wasm path/to/module.wasm \
  --input path/to/input.bin \
  --payment 1000000

# Check job status
certus status <jobId>

# Finalize completed job
certus finalize <jobId>

# Register Wasm module
certus register-wasm path/to/module.wasm
```

## Configuration

Create `~/.certus/config.json`:

```json
{
  "rpcUrl": "https://sepolia-rollup.arbitrum.io/rpc",
  "escrowAddress": "0x...",
  "privateKey": "0x..."
}
```

## Development

```bash
npm install
npm run build
npm test
```
