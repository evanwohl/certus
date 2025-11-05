#!/bin/bash
# Compile WAT files to WASM bytecode

# Install wabt tools if not present
if ! command -v wat2wasm &> /dev/null; then
    echo "Installing wabt tools..."
    npm install -g wabt
fi

# Compile all .wat files
for wat in *.wat; do
    wasm="${wat%.wat}.wasm"
    echo "Compiling $wat -> $wasm"
    wat2wasm "$wat" -o "$wasm"
done

echo "Compilation complete"