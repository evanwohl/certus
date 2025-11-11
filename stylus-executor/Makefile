# Certus Stylus Executor Makefile

.PHONY: all build test clean deploy-sepolia deploy-mainnet verify

all: build test

# Build for Wasm target
build:
	cargo build --release --target wasm32-unknown-unknown
	@echo "Build complete. Wasm size:"
	@ls -lh target/wasm32-unknown-unknown/release/certus_stylus_executor.wasm

# Run unit tests
test:
	cargo test --lib

# Run integration tests
test-integration:
	cargo test --test integration

# Clean build artifacts
clean:
	cargo clean
	rm -rf target/

# Check Wasm size
check-size:
	@wasm-opt --version || (echo "Install wasm-opt: npm install -g binaryen" && exit 1)
	wasm-opt -Oz target/wasm32-unknown-unknown/release/certus_stylus_executor.wasm -o optimized.wasm
	@echo "Original size:"
	@ls -lh target/wasm32-unknown-unknown/release/certus_stylus_executor.wasm
	@echo "Optimized size:"
	@ls -lh optimized.wasm

# Export ABI for Solidity integration
export-abi:
	cargo stylus export-abi

# Deploy to Arbitrum Sepolia
deploy-sepolia:
	@test -n "$(PRIVATE_KEY)" || (echo "Set PRIVATE_KEY env var" && exit 1)
	cargo stylus deploy \
		--private-key $(PRIVATE_KEY) \
		--endpoint https://sepolia-rollup.arbitrum.io/rpc

# Deploy to Arbitrum Mainnet (requires confirmation)
deploy-mainnet:
	@test -n "$(PRIVATE_KEY)" || (echo "Set PRIVATE_KEY env var" && exit 1)
	@echo "WARNING: Deploying to MAINNET. Press Ctrl+C to cancel, Enter to continue."
	@read
	cargo stylus deploy \
		--private-key $(PRIVATE_KEY) \
		--endpoint https://arb1.arbitrum.io/rpc

# Verify deployment on Arbiscan
verify:
	@test -n "$(DEPLOYMENT_TX)" || (echo "Set DEPLOYMENT_TX env var" && exit 1)
	cargo stylus verify \
		--deployment-tx $(DEPLOYMENT_TX) \
		--endpoint https://sepolia-rollup.arbitrum.io/rpc

# Dry run (estimate gas)
dry-run:
	@test -n "$(PRIVATE_KEY)" || (echo "Set PRIVATE_KEY env var" && exit 1)
	cargo stylus deploy \
		--private-key $(PRIVATE_KEY) \
		--endpoint https://sepolia-rollup.arbitrum.io/rpc \
		--dry-run

# Format code
fmt:
	cargo fmt

# Lint code
lint:
	cargo clippy -- -D warnings

# Security audit
audit:
	cargo audit

# Generate docs
docs:
	cargo doc --no-deps --open

# Run all checks before deployment
pre-deploy: fmt lint test audit
	@echo "All checks passed. Ready to deploy."
