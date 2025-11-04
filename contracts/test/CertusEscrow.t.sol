// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import "forge-std/Test.sol";
import "./CertusEscrow.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/**
 * Mock ERC20 token for testing
 */
contract MockUSDC is ERC20 {
    constructor() ERC20("Mock USDC", "USDC") {
        _mint(msg.sender, 1_000_000 * 10**6); // 1M USDC
    }

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

/**
 * Certus Escrow Contract Tests
 *
 * Tests all state transitions and security properties:
 * 1. Job lifecycle (create → accept → receipt → finalize)
 * 2. Timeout claims
 * 3. Fraud detection and slashing
 * 4. Economic security invariants
 */
contract CertusEscrowTest is Test {
    CertusEscrow public escrow;
    MockUSDC public usdc;

    address public owner;
    address public client;
    address public executor;
    address public verifier;

    bytes32 public constant WASM_HASH = bytes32(uint256(1));
    bytes32 public constant INPUT_HASH = bytes32(uint256(2));
    bytes32 public constant OUTPUT_HASH = bytes32(uint256(3));
    bytes32 public constant JOB_ID = bytes32(uint256(100));

    uint256 public constant PAY_AMT = 5 * 10**6; // 5 USDC (max allowed by executor collateral)
    uint256 public constant CLIENT_DEPOSIT = 5 * 10**6; // 5 USDC
    uint256 public constant EXECUTOR_DEPOSIT = 5 * 10**6; // 5 USDC

    function setUp() public {
        owner = address(this);
        client = makeAddr("client");
        executor = makeAddr("executor");
        verifier = makeAddr("verifier");

        // Deploy contracts
        escrow = new CertusEscrow();
        usdc = new MockUSDC();

        // Fund accounts
        usdc.mint(client, 1000 * 10**6);
        usdc.mint(executor, 1000 * 10**6);
        usdc.mint(verifier, 1000 * 10**6);

        // Register test wasm
        bytes memory testWasm = abi.encodePacked(
            bytes4(0x0061736D), // Magic: \0asm
            bytes4(0x01000000)  // Version: 1
        );
        escrow.registerWasm(testWasm);
    }

    // ============================================================================
    // Wasm Registration Tests
    // ============================================================================

    function testRegisterWasm() public {
        bytes memory wasm = abi.encodePacked(bytes4(0x0061736D), bytes4(0x01000000));
        bytes32 hash = escrow.registerWasm(wasm);

        assertEq(hash, sha256(wasm), "Wasm hash incorrect");

        bytes memory retrieved = escrow.wasmOf(hash);
        assertEq(keccak256(retrieved), keccak256(wasm), "Retrieved wasm doesn't match");
    }

    function testRegisterWasmTooLarge() public {
        bytes memory largeWasm = new bytes(25 * 1024); // 25KB
        vm.expectRevert("Invalid wasm size");
        escrow.registerWasm(largeWasm);
    }

    function testRegisterWasmDuplicate() public {
        bytes memory wasm = abi.encodePacked(bytes4(0x0061736D), bytes4(0x01000000));
        escrow.registerWasm(wasm);

        vm.expectRevert("Wasm already registered");
        escrow.registerWasm(wasm);
    }

    // ============================================================================
    // Job Creation Tests
    // ============================================================================

    function testCreateJob() public {
        vm.startPrank(client);
        usdc.approve(address(escrow), PAY_AMT + CLIENT_DEPOSIT);

        escrow.createJob(
            JOB_ID,
            address(usdc),
            PAY_AMT,
            WASM_HASH,
            INPUT_HASH,
            uint64(block.timestamp + 120),
            uint64(block.timestamp + 420),
            1_000_000, // fuelLimit
            64 * 1024 * 1024, // memLimit
            1024 * 1024 // maxOutputSize
        );
        vm.stopPrank();

        CertusEscrow.Job memory job = escrow.getJob(JOB_ID);
        assertEq(job.client, client);
        assertEq(job.payAmt, PAY_AMT);
        assertEq(uint(job.status), uint(CertusEscrow.Status.Created));
    }

    function testCreateJobInvalidParams() public {
        vm.startPrank(client);
        usdc.approve(address(escrow), PAY_AMT + CLIENT_DEPOSIT);

        // Invalid deadlines
        vm.expectRevert("Finalize must be after accept");
        escrow.createJob(
            JOB_ID,
            address(usdc),
            PAY_AMT,
            WASM_HASH,
            INPUT_HASH,
            uint64(block.timestamp + 420),
            uint64(block.timestamp + 120), // Finalize before accept
            1_000_000,
            64 * 1024 * 1024,
            1024 * 1024
        );
        vm.stopPrank();
    }

    // ============================================================================
    // Job Acceptance Tests
    // ============================================================================

    function testAcceptJob() public {
        _createJob();

        vm.startPrank(executor);
        usdc.approve(address(escrow), EXECUTOR_DEPOSIT);
        escrow.acceptJob(JOB_ID);
        vm.stopPrank();

        CertusEscrow.Job memory job = escrow.getJob(JOB_ID);
        assertEq(job.executor, executor);
        assertEq(uint(job.status), uint(CertusEscrow.Status.Accepted));
    }

    function testAcceptJobAfterDeadline() public {
        _createJob();

        // Advance time past accept deadline
        vm.warp(block.timestamp + 121);

        vm.startPrank(executor);
        usdc.approve(address(escrow), EXECUTOR_DEPOSIT);
        vm.expectRevert("Accept deadline passed");
        escrow.acceptJob(JOB_ID);
        vm.stopPrank();
    }

    // ============================================================================
    // Receipt Submission Tests
    // ============================================================================

    function testSubmitReceipt() public {
        _createJob();
        _acceptJob();

        bytes memory sig = new bytes(64); // Mock signature

        vm.prank(executor);
        escrow.submitReceipt(JOB_ID, OUTPUT_HASH, sig);

        CertusEscrow.Job memory job = escrow.getJob(JOB_ID);
        assertEq(job.outputHash, OUTPUT_HASH);
        assertEq(uint(job.status), uint(CertusEscrow.Status.Receipt));
    }

    // ============================================================================
    // Finalization Tests
    // ============================================================================

    function testFinalizeJob() public {
        _createJob();
        _acceptJob();
        _submitReceipt();

        uint256 executorBalanceBefore = usdc.balanceOf(executor);
        uint256 clientBalanceBefore = usdc.balanceOf(client);

        vm.prank(client);
        escrow.finalize(JOB_ID);

        // Executor should receive: payAmt - protocolFee + executorDeposit
        uint256 protocolFee = (PAY_AMT * 1) / 100;
        uint256 expectedExecutorPayment = PAY_AMT - protocolFee + EXECUTOR_DEPOSIT;

        assertEq(usdc.balanceOf(executor), executorBalanceBefore + expectedExecutorPayment);
        assertEq(usdc.balanceOf(client), clientBalanceBefore + CLIENT_DEPOSIT);

        CertusEscrow.Job memory job = escrow.getJob(JOB_ID);
        assertEq(uint(job.status), uint(CertusEscrow.Status.Finalized));
    }

    function testFinalizeAfterDeadline() public {
        _createJob();
        _acceptJob();
        _submitReceipt();

        // Advance past finalize deadline
        vm.warp(block.timestamp + 301);

        vm.prank(client);
        vm.expectRevert("Finalize deadline passed");
        escrow.finalize(JOB_ID);
    }

    // ============================================================================
    // Timeout Claim Tests
    // ============================================================================

    function testClaimTimeout() public {
        _createJob();
        _acceptJob();
        _submitReceipt();

        // Advance past finalize deadline
        vm.warp(block.timestamp + 301);

        uint256 executorBalanceBefore = usdc.balanceOf(executor);

        vm.prank(executor);
        escrow.claimTimeout(JOB_ID);

        uint256 protocolFee = (PAY_AMT * 1) / 100;
        uint256 expectedPayment = PAY_AMT - protocolFee + EXECUTOR_DEPOSIT;

        assertEq(usdc.balanceOf(executor), executorBalanceBefore + expectedPayment);
    }

    function testClaimTimeoutTooEarly() public {
        _createJob();
        _acceptJob();
        _submitReceipt();

        vm.prank(executor);
        vm.expectRevert("Finalize deadline not passed");
        escrow.claimTimeout(JOB_ID);
    }

    // ============================================================================
    // Fraud Detection Tests
    // ============================================================================

    function testFraudOnChain() public {
        _createJob();
        _acceptJob();

        // Submit INCORRECT receipt (wrong output hash)
        bytes memory input = "test input";
        bytes32 correctOutputHash = sha256(abi.encodePacked(sha256(input))); // What it should be
        bytes32 wrongOutputHash = sha256("wrong output"); // What executor claims

        // Executor submits fraudulent receipt
        bytes memory sig = new bytes(64);
        vm.prank(executor);
        escrow.submitReceipt(JOB_ID, wrongOutputHash, sig);

        // Verify initial balances
        uint256 verifierBalanceBefore = usdc.balanceOf(verifier);
        uint256 clientBalanceBefore = usdc.balanceOf(client);

        // Verifier detects fraud and submits proof
        bytes memory wasm = abi.encodePacked(bytes4(0x0061736D), bytes4(0x01000000));
        bytes memory claimedOutput = "wrong output";

        vm.prank(verifier);
        escrow.fraudOnChain(JOB_ID, wasm, input, claimedOutput);

        // Verify fraud was detected and executor slashed
        CertusEscrow.Job memory job = escrow.getJob(JOB_ID);
        assertEq(uint(job.status), uint(CertusEscrow.Status.Slashed));

        // Verify verifier received bounty
        uint256 totalSlashed = PAY_AMT + EXECUTOR_DEPOSIT;
        uint256 expectedBounty = (totalSlashed * 20) / 100;
        assertEq(usdc.balanceOf(verifier), verifierBalanceBefore + expectedBounty);

        // Verify client received refund
        uint256 expectedClientRefund = totalSlashed - expectedBounty + CLIENT_DEPOSIT;
        assertEq(usdc.balanceOf(client), clientBalanceBefore + expectedClientRefund);
    }

    // ============================================================================
    // Helper Functions
    // ============================================================================

    function _createJob() internal {
        vm.startPrank(client);
        usdc.approve(address(escrow), PAY_AMT + CLIENT_DEPOSIT);
        escrow.createJob(
            JOB_ID,
            address(usdc),
            PAY_AMT,
            WASM_HASH,
            INPUT_HASH,
            uint64(block.timestamp + 120),
            uint64(block.timestamp + 420),
            1_000_000,
            64 * 1024 * 1024,
            1024 * 1024
        );
        vm.stopPrank();
    }

    function _acceptJob() internal {
        vm.startPrank(executor);
        usdc.approve(address(escrow), EXECUTOR_DEPOSIT);
        escrow.acceptJob(JOB_ID);
        vm.stopPrank();
    }

    function _submitReceipt() internal {
        bytes memory sig = new bytes(64);
        vm.prank(executor);
        escrow.submitReceipt(JOB_ID, OUTPUT_HASH, sig);
    }
}
