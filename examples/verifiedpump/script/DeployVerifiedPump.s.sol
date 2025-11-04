// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/VerifiedPumpFactory.sol";

contract DeployVerifiedPump is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address certusEscrow = vm.envAddress("CERTUS_ESCROW_ADDRESS");

        // Arbitrum Sepolia addresses
        address uniswapRouter = 0x101F443B4d1b059569D643917553c771E1b9663E;
        address uniswapFactory = 0xF62849F9A0B5Bf2913b396098F7c7019b51A820a;
        address weth = 0x980B62Da83eFf3D4576C647993b0c1D7faf17c73;

        // Deploy analyzer wasm first and get hash
        bytes32 analyzerHash = keccak256("PLACEHOLDER_ANALYZER_HASH");

        vm.startBroadcast(deployerPrivateKey);

        VerifiedPumpFactory factory = new VerifiedPumpFactory(
            certusEscrow,
            uniswapRouter,
            uniswapFactory,
            weth,
            analyzerHash
        );

        vm.stopBroadcast();

        console.log("VerifiedPumpFactory deployed at:", address(factory));
    }
}
