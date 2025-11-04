#!/bin/bash
set -e

# Compile test Wasm modules from WAT
# Requires: wat2wasm (from WABT toolkit)

echo "Compiling test Wasm modules..."

if ! command -v wat2wasm &> /dev/null; then
    echo "Error: wat2wasm not found"
    echo "Install WABT toolkit: https://github.com/WebAssembly/wabt"
    echo ""
    echo "On macOS: brew install wabt"
    echo "On Ubuntu: sudo apt-get install wabt"
    exit 1
fi

cd testvectors/wasm

# Compile echo.wat
if [ -f "echo.wat" ]; then
    echo "Compiling echo.wat..."
    wat2wasm echo.wat -o echo.wasm
    echo "✓ echo.wasm created ($(wc -c < echo.wasm) bytes)"
else
    echo "Warning: echo.wat not found"
fi

echo ""
echo "Test Wasm modules compiled successfully!"
echo ""
echo "SHA256 hashes:"
for file in *.wasm; do
    if [ -f "$file" ]; then
        hash=$(sha256sum "$file" | cut -d' ' -f1)
        echo "  $file: $hash"
    fi
done
