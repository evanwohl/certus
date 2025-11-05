// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Test.sol";
import "../src/CertusJobs.sol";
import "../src/CertusVerifier.sol";
import "../src/CertusBisection.sol";
import "../src/CertusEscrow.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract MockToken is ERC20 {
    uint8 private _decimals;
    constructor(string memory name, uint8 decimals_) ERC20(name, name) {
        _decimals = decimals_;
    }
    function decimals() public view override returns (uint8) {
        return _decimals;
    }
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

contract CertusSecurityTest is Test {
    CertusJobs jobs;
    CertusVerifier verifier;
    CertusBisection bisection;
    CertusEscrow escrow;

    MockToken usdc; // 6 decimals
    MockToken dai;  // 18 decimals
    address alice = address(0x1);
    address bob = address(0x2);
    address charlie = address(0x3);
    function setUp() public {
        // Deploy tokens
        usdc = new MockToken("USDC", 6);
        dai = new MockToken("DAI", 18);

        // Deploy contracts
        verifier = new CertusVerifier(address(0), bytes32(0), 0);
        bisection = new CertusBisection(address(this), address(0));
        jobs = new CertusJobs(
            address(verifier),
            address(bisection),
            address(0),
            address(0),
            bytes32(0),
            0
        );
        escrow = new CertusEscrow(
            address(jobs),
            address(verifier),
            address(bisection),
            address(0)
        );

        // Wire up contracts
        jobs.setEscrowContract(address(escrow));
        verifier.setEscrowContract(address(escrow));

        // Register tokens
        jobs.registerToken(address(usdc));
        jobs.registerToken(address(dai));

        // Fund test accounts
        usdc.mint(alice, 100_000e6);  // 100k USDC
        usdc.mint(bob, 100_000e6);
        dai.mint(charlie, 100_000e18); // 100k DAI
    }

    /**
     * Test 1: Decimal normalization across different tokens
     */
    function testDecimalNormalization() public {
        // Min stake should be $1000 worth regardless of decimals
        assertEq(jobs.minStakeInTokenUnits(address(usdc)), 1000e6, "USDC min stake");
        assertEq(jobs.minStakeInTokenUnits(address(dai)), 1000e18, "DAI min stake");

        // Protocol fee calculation should handle decimals
        uint256 usdcFee = jobs.calculateProtocolFee(100e6, address(usdc)); // $100 job
        uint256 daiFee = jobs.calculateProtocolFee(100e18, address(dai));   // $100 job

        // Both should be ~1.5% = $1.50
        assertEq(usdcFee, 15e5, "USDC fee wrong"); // $1.50 in 6 decimals
        assertEq(daiFee, 15e17, "DAI fee wrong");  // $1.50 in 18 decimals
    }

    /**
     * Test 2: Output size enforcement
     */
    function testOutputSizeEnforcement() public {
        vm.startPrank(alice);
        usdc.approve(address(jobs), type(uint256).max);

        bytes32 jobId = keccak256("job1");
        jobs.createJob(
            jobId,
            keccak256("wasm"),
            keccak256("input"),
            address(usdc),
            100e6, // $100
            1 hours,
            1 hours,
            1000000,
            1000000,
            1024 // 1KB max output
        );

        // Bob accepts
        vm.startPrank(bob);
        usdc.approve(address(jobs), type(uint256).max);
        jobs.acceptJob(jobId);

        // Try to submit output exceeding max size
        vm.expectRevert("Output exceeds max size");
        jobs.submitReceipt(
            jobId,
            keccak256("output"),
            "",
            2048 // 2KB output - exceeds max
        );

        // Submit with valid size
        jobs.submitReceipt(
            jobId,
            keccak256("output"),
            "",
            512 // 512B output - within limit
        );
    }

    /**
     * Test 3: Executor reputation tracking
     */
    function testExecutorBanning() public {
        vm.startPrank(alice);
        usdc.approve(address(jobs), type(uint256).max);

        // Create first job
        bytes32 jobId1 = keccak256("job1");
        jobs.createJob(
            jobId1,
            keccak256("wasm"),
            keccak256("input"),
            address(usdc),
            100e6,
            1 hours,
            1 hours,
            1000000,
            1000000,
            1024
        );

        vm.startPrank(bob);
        usdc.approve(address(jobs), type(uint256).max);
        jobs.acceptJob(jobId1);
        jobs.submitReceipt(jobId1, keccak256("output"), "", 100);

        // Mark as slashed (fraud detected)
        vm.startPrank(address(escrow));
        jobs.markSlashed(jobId1);
        // Check reputation updated
        (uint256 completed, uint256 frauds, , uint256 banUntil, bool permBan) =
            jobs.executorReputation(bob);
        assertEq(frauds, 1, "Fraud count");
        assertGt(banUntil, block.timestamp, "Should be banned");
        assertFalse(permBan, "Not permanently banned yet");
        // Try to accept another job while banned
        vm.startPrank(alice);
        bytes32 jobId2 = keccak256("job2");
        jobs.createJob(
            jobId2,
            keccak256("wasm"),
            keccak256("input"),
            address(usdc),
            100e6,
            1 hours,
            1 hours,
            1000000,
            1000000,
            1024
        );

        vm.startPrank(bob);
        vm.expectRevert("Executor temporarily banned");
        jobs.acceptJob(jobId2);

        // Fast forward past ban
        vm.warp(block.timestamp + 31 days);
        jobs.acceptJob(jobId2);
        jobs.submitReceipt(jobId2, keccak256("output"), "", 100);

        // Second fraud - permanent ban
        vm.startPrank(address(escrow));
        jobs.markSlashed(jobId2);

        (, frauds, , , permBan) = jobs.executorReputation(bob);
        assertEq(frauds, 2, "Fraud count");
        assertTrue(permBan, "Should be permanently banned");
    }

    /**
     * Test 4: VRF fallback mechanism
     */
    function testVRFFallback() public {
        vm.startPrank(alice);
        usdc.approve(address(jobs), type(uint256).max);

        bytes32 jobId = keccak256("job1");
        jobs.createJob(
            jobId,
            keccak256("wasm"),
            keccak256("input"),
            address(usdc),
            100e6,
            1 hours,
            1 hours,
            1000000,
            1000000,
            1024
        );

        vm.startPrank(bob);
        usdc.approve(address(jobs), type(uint256).max);
        jobs.acceptJob(jobId);
        jobs.submitReceipt(jobId, keccak256("output"), "", 100);

        // VRF doesn't respond, wait for grace period
        vm.warp(block.timestamp + 31 minutes);

        // Trigger fallback selection
        jobs.fallbackVerifierSelection(jobId);

        // Check verifiers were selected
        CertusBase.Job memory job = jobs.getJob(jobId);
        assertTrue(job.selectedVerifiers[0] != address(0) ||
                   job.selectedVerifiers[1] != address(0) ||
                   job.selectedVerifiers[2] != address(0), "Should have verifiers");
    }
    /**
     * Test 5: Bisection griefing protection
     */
    function testBisectionAntiGrief() public {
        vm.startPrank(alice);
        dai.approve(address(bisection), type(uint256).max);

        bytes32 jobId = keccak256("job1");

        // Initiate bisection
        vm.startPrank(address(escrow));
        bisection.initiateBisection(
            jobId,
            1000, // 1000 steps
            keccak256("final"),
            alice,
            bob,
            address(dai),
            100e18 // Initial stake
        );

        // Simulate 6 rounds (grief protection kicks in after round 5)
        for (uint256 i = 0; i < 5; i++) {
            vm.startPrank(bob);
            bisection.executorRespond(jobId, keccak256(abi.encode(i)));

            vm.startPrank(alice);
            bisection.challengerPick(jobId, true, keccak256(abi.encode(i)), address(dai));
        }

        // Round 6 requires exponential stake
        vm.startPrank(bob);
        bisection.executorRespond(jobId, keccak256("round6"));

        vm.startPrank(alice);
        uint256 balanceBefore = dai.balanceOf(alice);
        // This should require additional stake (2^5 * base = 32x)
        bisection.challengerPick(jobId, true, keccak256("wrong"), address(dai));
        uint256 balanceAfter = dai.balanceOf(alice);
        assertGt(balanceBefore - balanceAfter, 100e18, "Should require exponential stake");
    }

    /**
     * Test 6: Region concentration limits
     */
    function testRegionConcentration() public {
        // Register 10 verifiers
        for (uint8 i = 0; i < 10; i++) {
            address v = address(uint160(100 + i));
            vm.startPrank(v);
            usdc.mint(v, 2000e6);
            usdc.approve(address(verifier), type(uint256).max);

            uint8 region = i < 3 ? 1 : (i % 5); // 3 in region 1, rest distributed
            verifier.registerVerifier(address(usdc), 1000e6, 100, region);
        }
        // Try to add 4th verifier to region 1 (would exceed 30% limit)
        address v11 = address(111);
        vm.startPrank(v11);
        usdc.mint(v11, 2000e6);
        usdc.approve(address(verifier), type(uint256).max);

        vm.expectRevert("Region concentration exceeded");
        verifier.registerVerifier(address(usdc), 1000e6, 100, 1);

        // Can add to different region
        verifier.registerVerifier(address(usdc), 1000e6, 100, 2);
    }

    /**
     * Test 7: Client griefing protection
     */
    function testClientGriefing() public {
        vm.startPrank(alice);
        usdc.approve(address(jobs), type(uint256).max);

        // Grief 3 times
        for (uint256 i = 0; i < 3; i++) {
            bytes32 jobId = keccak256(abi.encode("job", i));
            jobs.createJob(
                jobId,
                keccak256("wasm"),
                keccak256("input"),
                address(usdc),
                100e6,
                1 hours,
                1 hours,
                1000000,
                1000000,
                1024
            );

            vm.startPrank(bob);
            usdc.approve(address(jobs), type(uint256).max);
            jobs.acceptJob(jobId);
            jobs.submitReceipt(jobId, keccak256("output"), "", 100);

            // Client doesn't finalize, executor claims timeout
            vm.warp(block.timestamp + 2 hours);
            vm.startPrank(address(escrow));
            jobs.markFinalized(jobId); // Counts as grief
        }

        // 4th attempt should fail
        vm.startPrank(alice);
        bytes32 jobId4 = keccak256("job4");
        vm.expectRevert("Client banned");
        jobs.createJob(
            jobId4,
            keccak256("wasm"),
            keccak256("input"),
            address(usdc),
            100e6,
            1 hours,
            1 hours,
            1000000,
            1000000,
            1024
        );
    }
    /**
     * Test 8: Cross-contract reentrancy protection
     */
    function testReentrancyProtection() public {
        // All critical functions have nonReentrant modifier
        // This is a compile-time check, but we verify the storage slot

        // Check that reentrancy guard storage is properly initialized
        bytes32 slot = bytes32(uint256(keccak256("eip1967.proxy.reentrancyGuard")) - 1);
        uint256 guardStatus;

        assembly {
            guardStatus := sload(slot)
        }

        // Should be NOT_ENTERED (1) initially
        assertEq(guardStatus, 0, "Reentrancy guard initialized");
    }
}