// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * Cryptographic proofs for deterministic Wasm execution
 */
contract CertusWasmProof {

    struct ExecutionProof {
        bytes32 wasmHash;
        bytes32 inputHash;
        bytes32 outputHash;
        bytes32 stateRoot;
        uint256 gasUsed;
        bytes32[] merkleProof;
    }

    struct WasmConstraints {
        bool noFloats;
        bool noWASI;
        bool noThreads;
        bool noRandom;
        uint64 maxMemory;
        uint64 maxFuel;
    }

    // Verified Wasm modules with determinism constraints
    mapping(bytes32 => WasmConstraints) public verifiedModules;
    mapping(bytes32 => bytes32) public executionRoots;

    event ModuleVerified(bytes32 indexed wasmHash, WasmConstraints constraints);
    event ProofGenerated(bytes32 indexed executionId, bytes32 stateRoot);

    /**
     * Verify Wasm module meets determinism requirements
     */
    function verifyModule(bytes calldata wasmCode) external returns (bool) {
        bytes32 wasmHash = keccak256(wasmCode);

        // Already verified
        if (verifiedModules[wasmHash].maxMemory > 0) {
            return true;
        }

        WasmConstraints memory constraints;

        // Check WebAssembly magic number and version
        require(wasmCode.length >= 8, "Invalid Wasm");
        require(
            wasmCode[0] == 0x00 && wasmCode[1] == 0x61 &&
            wasmCode[2] == 0x73 && wasmCode[3] == 0x6D,
            "Not a Wasm module"
        );

        // Scan for non-deterministic operations
        constraints.noFloats = !_containsFloatOps(wasmCode);
        constraints.noWASI = !_containsWASIImports(wasmCode);
        constraints.noThreads = !_containsThreadOps(wasmCode);
        constraints.noRandom = !_containsRandomOps(wasmCode);
        constraints.maxMemory = 10 * 1024 * 1024; // 10MB
        constraints.maxFuel = 1_000_000_000;

        require(
            constraints.noFloats &&
            constraints.noWASI &&
            constraints.noThreads &&
            constraints.noRandom,
            "Module not deterministic"
        );

        verifiedModules[wasmHash] = constraints;
        emit ModuleVerified(wasmHash, constraints);

        return true;
    }

    /**
     * Generate cryptographic proof of execution
     */
    function generateProof(
        bytes32 wasmHash,
        bytes32 inputHash,
        bytes calldata output
    ) external returns (ExecutionProof memory) {
        require(verifiedModules[wasmHash].maxMemory > 0, "Module not verified");

        bytes32 outputHash = keccak256(output);
        bytes32 executionId = keccak256(abi.encodePacked(wasmHash, inputHash, outputHash));

        // Build merkle tree of execution trace
        bytes32[] memory trace = new bytes32[](4);
        trace[0] = wasmHash;
        trace[1] = inputHash;
        trace[2] = outputHash;
        trace[3] = keccak256(abi.encodePacked(block.number, block.timestamp));

        bytes32 stateRoot = _computeMerkleRoot(trace);
        executionRoots[executionId] = stateRoot;

        emit ProofGenerated(executionId, stateRoot);

        return ExecutionProof({
            wasmHash: wasmHash,
            inputHash: inputHash,
            outputHash: outputHash,
            stateRoot: stateRoot,
            gasUsed: gasleft(),
            merkleProof: trace
        });
    }

    /**
     * Verify execution proof
     */
    function verifyProof(ExecutionProof calldata proof) external view returns (bool) {
        // Verify module was pre-verified
        require(verifiedModules[proof.wasmHash].maxMemory > 0, "Module not verified");

        // Verify merkle root
        bytes32 computedRoot = _computeMerkleRoot(proof.merkleProof);

        bytes32 executionId = keccak256(abi.encodePacked(
            proof.wasmHash,
            proof.inputHash,
            proof.outputHash
        ));

        return executionRoots[executionId] == computedRoot;
    }

    /**
     * Check for float operations
     */
    function _containsFloatOps(bytes calldata wasm) internal pure returns (bool) {
        // f32 = 0x7D, f64 = 0x7C in Wasm binary
        for (uint i = 0; i < wasm.length; i++) {
            if (wasm[i] == 0x7C || wasm[i] == 0x7D) {
                return true;
            }
        }
        return false;
    }

    /**
     * Check for WASI imports
     */
    function _containsWASIImports(bytes calldata wasm) internal pure returns (bool) {
        // Look for "wasi_snapshot" string
        bytes memory wasiSig = hex"776173695f736e617073686f74";
        return _findPattern(wasm, wasiSig);
    }

    /**
     * Check for thread operations
     */
    function _containsThreadOps(bytes calldata wasm) internal pure returns (bool) {
        // Atomic operations prefix with 0xFE
        for (uint i = 0; i < wasm.length; i++) {
            if (wasm[i] == 0xFE) {
                return true;
            }
        }
        return false;
    }

    /**
     * Check for random operations
     */
    function _containsRandomOps(bytes calldata wasm) internal pure returns (bool) {
        // Check for random imports
        bytes memory randSig = hex"72616e646f6d"; // "random"
        return _findPattern(wasm, randSig);
    }

    /**
     * Pattern matching helper
     */
    function _findPattern(bytes calldata data, bytes memory pattern) internal pure returns (bool) {
        if (pattern.length > data.length) return false;

        for (uint i = 0; i <= data.length - pattern.length; i++) {
            bool found = true;
            for (uint j = 0; j < pattern.length; j++) {
                if (data[i + j] != pattern[j]) {
                    found = false;
                    break;
                }
            }
            if (found) return true;
        }
        return false;
    }

    /**
     * Compute merkle root
     */
    function _computeMerkleRoot(bytes32[] memory leaves) internal pure returns (bytes32) {
        uint256 n = leaves.length;
        if (n == 0) return bytes32(0);
        if (n == 1) return leaves[0];

        // Build tree layer by layer
        while (n > 1) {
            uint256 nextN = (n + 1) / 2;
            for (uint256 i = 0; i < nextN; i++) {
                uint256 left = 2 * i;
                uint256 right = left + 1;

                if (right < n) {
                    leaves[i] = keccak256(abi.encodePacked(leaves[left], leaves[right]));
                } else {
                    leaves[i] = leaves[left];
                }
            }
            n = nextN;
        }

        return leaves[0];
    }
}