// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./IStylusExecutor.sol";

/**
 * @title StylusExecutorMultisig
 * @notice Multi-implementation Stylus executor for trust minimization
 * @dev Requires 2-of-3 implementations to agree on output
 */
contract StylusExecutorMultisig is IStylusExecutor {

    IStylusExecutor public immutable impl1;
    IStylusExecutor public immutable impl2;
    IStylusExecutor public immutable impl3;

    constructor(address _impl1, address _impl2, address _impl3) {
        require(_impl1 != address(0) && _impl2 != address(0) && _impl3 != address(0), "Invalid implementations");
        impl1 = IStylusExecutor(_impl1);
        impl2 = IStylusExecutor(_impl2);
        impl3 = IStylusExecutor(_impl3);
    }

    function execute(
        bytes calldata wasm,
        bytes calldata input,
        uint64 fuelLimit,
        uint64 memLimit
    ) external override returns (bytes memory) {
        // Execute on all three implementations
        bytes memory output1 = impl1.execute(wasm, input, fuelLimit, memLimit);
        bytes memory output2 = impl2.execute(wasm, input, fuelLimit, memLimit);
        bytes memory output3 = impl3.execute(wasm, input, fuelLimit, memLimit);

        // Require 2-of-3 agreement
        if (keccak256(output1) == keccak256(output2)) {
            return output1;
        } else if (keccak256(output1) == keccak256(output3)) {
            return output1;
        } else if (keccak256(output2) == keccak256(output3)) {
            return output2;
        } else {
            revert("No consensus among executors");
        }
    }
}