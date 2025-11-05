// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * @title IStylusExecutor
 * @notice Interface for Arbitrum Stylus WebAssembly execution
 * @dev CRITICAL: Implementation must be formally verified
 */
interface IStylusExecutor {
    /**
     * Execute WebAssembly module with given input
     * @param wasm Module bytecode
     * @param input Input data
     * @param fuelLimit Maximum instructions
     * @param memLimit Maximum memory in bytes
     * @return output Execution result
     */
    function execute(
        bytes calldata wasm,
        bytes calldata input,
        uint64 fuelLimit,
        uint64 memLimit
    ) external returns (bytes memory output);
}