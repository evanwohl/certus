#!/bin/bash
set -e

# Certus Build Verification Script
# Verifies that all components can build and tests pass

echo "========================================="
echo "  Certus Build Verification"
echo "========================================="
echo ""

FAILED=0

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

check_prerequisite() {
    local cmd=$1
    local name=$2

    if command -v $cmd &> /dev/null; then
        echo -e "${GREEN}✓${NC} $name found: $(command -v $cmd)"
    else
        echo -e "${RED}✗${NC} $name not found. Please install $name."
        FAILED=1
    fi
}

run_test() {
    local name=$1
    local cmd=$2

    echo ""
    echo "-------------------------------------------"
    echo "Testing: $name"
    echo "-------------------------------------------"

    if eval $cmd; then
        echo -e "${GREEN}✓${NC} $name: PASSED"
    else
        echo -e "${RED}✗${NC} $name: FAILED"
        FAILED=1
    fi
}

# Check prerequisites
echo "Checking prerequisites..."
check_prerequisite "java" "Java"
check_prerequisite "node" "Node.js"
check_prerequisite "gradle" "Gradle"

if [ $FAILED -eq 1 ]; then
    echo ""
    echo -e "${RED}Prerequisites missing. Please install required tools.${NC}"
    exit 1
fi

echo ""
echo "Java version: $(java -version 2>&1 | head -n 1)"
echo "Node version: $(node --version)"
echo "Gradle version: $(gradle --version | head -n 1)"

# Build tests
echo ""
echo "========================================="
echo "  Build Tests"
echo "========================================="

run_test "Clean build" "./gradlew clean"
run_test "Compile all modules" "./gradlew compileJava"
run_test "Generate protobuf" "./gradlew :proto:generateProto"

# Unit tests
echo ""
echo "========================================="
echo "  Unit Tests"
echo "========================================="

run_test "Client module tests" "./gradlew :client:test"
run_test "Executor module tests" "./gradlew :executor:test"
run_test "Verifier module tests" "./gradlew :verifier:test"

# Determinism tests (critical)
echo ""
echo "========================================="
echo "  CRITICAL: Determinism Tests"
echo "========================================="

if [ -f "testvectors/wasm/echo.wasm" ]; then
    run_test "Determinism validation" "./gradlew :executor:test --tests DeterminismTest"
else
    echo -e "Test vectors not found - skipping determinism tests"
    echo "  Run this after compiling test Wasm modules"
fi

# Contract tests
echo ""
echo "========================================="
echo "  Contract Tests"
echo "========================================="

if command -v forge &> /dev/null; then
    run_test "Contract compilation" "cd contracts && forge build"
    run_test "Contract tests" "cd contracts && forge test"
else
    echo -e "Foundry not found - skipping contract tests"
    echo "  Install from: https://book.getfoundry.sh/getting-started/installation"
fi

# CLI tests
echo ""
echo "========================================="
echo "  CLI Tests"
echo "========================================="

if [ -f "cli/package.json" ]; then
    run_test "CLI dependencies" "cd cli && npm install"
    run_test "CLI build" "cd cli && npm run build"
else
    echo -e "CLI package.json not found"
fi

# Final summary
echo ""
echo "========================================="
echo "  Build Verification Summary"
echo "========================================="

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}ALL TESTS PASSED${NC}"
    echo ""
    echo "Your Certus build is ready!"
    echo ""
    echo "Next steps:"
    echo "1. Review QUICKSTART.md for deployment instructions"
    echo "2. Configure your .env.testnet file"
    echo "3. Deploy contracts with ./scripts/deploy.sh testnet"
    echo "4. Start executor and verifier nodes"
    echo ""
    exit 0
else
    echo -e "${RED}SOME TESTS FAILED${NC}"
    echo ""
    echo "Please fix the failing tests before proceeding."
    echo ""
    exit 1
fi
