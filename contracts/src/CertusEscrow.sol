// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./CertusBase.sol";
import "./CertusJobs.sol";
import "./CertusVerifier.sol";
import "./CertusBisection.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

interface IStylusWasmExecutor {
    function execute(
        bytes calldata wasm,
        bytes calldata input,
        uint64 fuelLimit,
        uint64 memLimit
    ) external returns (bytes memory output);
}

/**
 * @title CertusEscrow
 * @notice Orchestrator contract for modular Certus protocol
 */
contract CertusEscrow is CertusBase, ReentrancyGuard, Ownable {
    using SafeERC20 for IERC20;

    // Module contracts
    CertusJobs public immutable jobsModule;
    CertusVerifier public immutable verifierModule;
    CertusBisection public immutable bisectionModule;
    address public immutable stylusExecutor;

    // Emergency pause
    bool public paused;

    modifier whenNotPaused() {
        require(!paused, "Contract paused");
        _;
    }

    /**
     * @param _stylusExecutor CRITICAL: Must be audited, immutable contract.
     *                        Consider CREATE2 deployment for deterministic address.
     */
    constructor(
        address _jobsModule,
        address _verifierModule,
        address _bisectionModule,
        address _stylusExecutor
    ) {
        jobsModule = CertusJobs(_jobsModule);
        verifierModule = CertusVerifier(_verifierModule);
        bisectionModule = CertusBisection(_bisectionModule);
        stylusExecutor = _stylusExecutor;
    }

    // MEV protection: commit hash first
    mapping(bytes32 => bytes32) public fraudCommitments;
    mapping(bytes32 => uint256) public fraudCommitTime;

    /**
     * Commit fraud proof hash (MEV protection step 1)
     */
    function commitFraud(bytes32 jobId, bytes32 commitment) external {
        Job memory job = jobsModule.getJob(jobId);
        require(job.status == Status.Receipt, "Not in receipt state");
        require(_isSelectedVerifier(jobId, msg.sender), "Not selected verifier");

        fraudCommitments[jobId] = commitment;
        fraudCommitTime[jobId] = block.timestamp;
    }

    /**
     * Reveal fraud proof after commit (MEV protection step 2)
     */
    function fraudOnChain(
        bytes32 jobId,
        bytes calldata wasm,
        bytes calldata input,
        bytes calldata claimedOutput,
        uint256 nonce
    ) external whenNotPaused nonReentrant {
        Job memory job = jobsModule.getJob(jobId);
        require(job.status == Status.Receipt, "Not in receipt state");

        // MEV protection: verify commitment
        bytes32 commitment = keccak256(abi.encodePacked(jobId, wasm, input, claimedOutput, nonce, msg.sender));
        require(fraudCommitments[jobId] == commitment, "Invalid commitment");
        require(block.timestamp >= fraudCommitTime[jobId] + 2 minutes, "Reveal too early");
        require(block.timestamp <= fraudCommitTime[jobId] + 10 minutes, "Reveal too late");

        // Handle VRF pending case
        if (job.selectedVerifiers[0] == address(0)) {
            require(
                block.timestamp > jobsModule.vrfRequestTime(jobId) + VRF_RETRY_GRACE_PERIOD,
                "VRF pending - call fallbackVerifierSelection first"
            );
            revert("No verifiers selected");
        }

        require(_isSelectedVerifier(jobId, msg.sender), "Not selected verifier");
        require(sha256(wasm) == job.wasmHash, "Wasm hash mismatch");
        require(sha256(input) == job.inputHash, "Input hash mismatch");

        // Gas optimization: force bisection for large jobs
        if (wasm.length > 1000 || input.length > 1000) {
            // Large job - must use bisection to avoid gas DOS
            revert("Job too large for direct proof - use initiateBisection");
        }

        // Small job - execute on-chain
        bytes memory recomputedOutput = _executeWasmOnChain(wasm, input, job.fuelLimit, job.memLimit);

        // Check for abort sentinel
        if (keccak256(recomputedOutput) == keccak256("STYLUS_ERROR")) {
            _handleAbort(jobId, job);
            return;
        }

        bytes32 recomputedHash = sha256(recomputedOutput);
        if (recomputedHash != job.outputHash) {
            _handleFraud(jobId, job, msg.sender);
        } else {
            revert("No fraud detected");
        }
    }

    /**
     * Initiate bisection challenge
     */
    function initiateBisection(
        bytes32 jobId,
        uint256 totalSteps,
        bytes32 finalStateRoot
    ) external whenNotPaused nonReentrant {
        Job memory job = jobsModule.getJob(jobId);
        require(job.status == Status.Receipt, "Not in receipt state");
        require(_isSelectedVerifier(jobId, msg.sender), "Not selected verifier");

        // Require challenge stake
        uint256 stake = _normalizeStake(CHALLENGE_STAKE, job.payToken);
        IERC20(job.payToken).safeTransferFrom(msg.sender, address(this), stake);

        // Delegate to bisection module
        bisectionModule.initiateBisection(
            jobId,
            totalSteps,
            finalStateRoot,
            msg.sender,
            job.executor,
            job.payToken,
            stake
        );
    }

    /**
     * Resolve bisection
     */
    function resolveBisection(
        bytes32 jobId,
        bytes calldata stepData,
        bytes32[] calldata preStateProof,
        bytes32[] calldata postStateProof
    ) external whenNotPaused nonReentrant {
        Job memory job = jobsModule.getJob(jobId);
        BisectionChallenge memory challenge = bisectionModule.getChallenge(jobId);
        require(challenge.challenger == msg.sender, "Not challenger");

        bool fraud = bisectionModule.resolveBisection(jobId, stepData, preStateProof, postStateProof);

        if (fraud) {
            _handleFraud(jobId, job, challenge.challenger);
            // Return challenge stake + refund bonds
            IERC20(job.payToken).safeTransfer(challenge.challenger, challenge.challengeStake);
            bisectionModule.refundBonds(jobId, true, job.payToken);
        } else {
            // No fraud - executor gets challenge stake + bonds
            IERC20(job.payToken).safeTransfer(job.executor, challenge.challengeStake);
            bisectionModule.refundBonds(jobId, false, job.payToken);
        }
    }

    /**
     * Slash non-responsive verifier
     */
    function slashVerifier(
        bytes32 jobId,
        address verifier
    ) external whenNotPaused nonReentrant {
        Job memory job = jobsModule.getJob(jobId);
        require(job.status == Status.Receipt, "Not in receipt state");
        require(_isSelectedVerifier(jobId, verifier), "Not selected verifier");
        uint256 timeSinceReceipt = block.timestamp - jobsModule.receiptTimestamp(jobId);
        require(timeSinceReceipt > VERIFIER_RESPONSE_DEADLINE, "Deadline not passed");
        // Slash 50% of verifier's actual stake (not min stake)
        uint256 actualStake = verifierModule.getVerifierStakeAmount(verifier);
        uint256 penalty = actualStake / 2; // 50% of actual stake
        verifierModule.slashVerifier(verifier, msg.sender, penalty);

        emit VerifierSlashed(jobId, verifier, msg.sender, penalty);
    }

    /**
     * Claim timeout
     */
    function claimTimeout(bytes32 jobId) external whenNotPaused nonReentrant {
        Job memory job = jobsModule.getJob(jobId);
        require(job.status == Status.Receipt, "Not in receipt state");
        require(msg.sender == job.executor, "Only executor");
        require(block.timestamp > job.finalizeDeadline, "Deadline not passed");

        // Check bisection status
        if (bisectionModule.isExecutorTimedOut(jobId)) {
            // Executor failed bisection - fraud
            _handleFraud(jobId, job, address(0));
        } else {
            // Client timeout - executor gets payment
            _handleTimeout(jobId, job);
        }
    }

    /**
     * Handle fraud detection
     */
    function _handleFraud(bytes32 jobId, Job memory job, address verifier) internal {
        uint256 totalSlashed = job.payAmt + job.executorDeposit;
        uint256 verifierBounty = (totalSlashed * VERIFIER_BOUNTY_PCT) / 100;
        uint256 clientRefund = totalSlashed - verifierBounty;

        // Update state in jobs module
        jobsModule.markSlashed(jobId);

        // Transfers - prevent self-payment
        if (verifier != address(0) && verifier != job.executor) {
            IERC20(job.payToken).safeTransfer(verifier, verifierBounty);
            IERC20(job.payToken).safeTransfer(job.client, clientRefund + job.clientDeposit);
        } else {
            // no bounty if executor self-challenges
            IERC20(job.payToken).safeTransfer(job.client, totalSlashed + job.clientDeposit);
        }

        emit FraudDetected(jobId, job.executor, verifier, totalSlashed);
    }

    /**
     * Handle timeout
     */
    function _handleTimeout(bytes32 jobId, Job memory job) internal {
        uint256 protocolFee = jobsModule.calculateProtocolFee(job.payAmt, job.payToken);
        uint256 executorPayment = job.payAmt - protocolFee + job.executorDeposit + job.clientDeposit;

        // Update state
        jobsModule.markFinalized(jobId);

        // Transfer
        IERC20(job.payToken).safeTransfer(job.executor, executorPayment);

        emit TimeoutClaimed(jobId, job.executor, executorPayment);
    }

    /**
     * Check if verifier selected
     */
    function _isSelectedVerifier(bytes32 jobId, address verifier) internal view returns (bool) {
        Job memory job = jobsModule.getJob(jobId);
        return verifierModule.isSelectedVerifier(jobId, verifier, job.selectedVerifiers);
    }

    /**
     * Execute WASM through Arbitrum Stylus
     */
    function _executeWasmOnChain(
        bytes calldata wasm,
        bytes calldata input,
        uint64 fuelLimit,
        uint64 memLimit
    ) internal returns (bytes memory) {
        // Direct call to Stylus - either precompile or deployed verifier
        address executor = stylusExecutor == address(0)
            ? address(0x0000000000000000000000000000000000000072)  // Native precompile
            : stylusExecutor;                                       // Deployed verifier

        // Single call with gas limit
        (bool success, bytes memory output) = executor.call{gas: uint256(fuelLimit) * 100}(
            abi.encode(wasm, input, memLimit)
        );

        return success ? output : bytes("STYLUS_ERROR");
    }

    /**
     * Normalize stake amount
     */
    function _normalizeStake(uint256 amount, address token) internal view returns (uint256) {
        uint8 decimals = jobsModule.tokenDecimals(token);
        return jobsModule.normalizeAmount(amount, 6, decimals);
    }

    /**
     * Handle aborted execution
     */
    function _handleAbort(bytes32 jobId, Job memory job) internal {
        // refund client, slash executor for invalid job
        jobsModule.markSlashed(jobId);

        // partial slash - 50% for abort vs 100% for fraud
        uint256 executorPenalty = job.executorDeposit / 2;
        uint256 clientRefund = job.payAmt + job.clientDeposit + executorPenalty;

        IERC20(job.payToken).safeTransfer(job.client, clientRefund);
        IERC20(job.payToken).safeTransfer(job.executor, job.executorDeposit - executorPenalty);

        emit JobAborted(jobId, job.executor, executorPenalty);
    }

    /**
     * Emergency pause
     */
    function pause() external onlyOwner whenNotPaused {
        paused = true;
        jobsModule.pause();
        emit Paused(msg.sender);
    }

    function unpause() external onlyOwner {
        require(paused, "Not paused");
        paused = false;
        jobsModule.unpause();
        emit Unpaused(msg.sender);
    }

    // Events
    event JobAborted(bytes32 indexed jobId, address indexed executor, uint256 penalty);
    event Paused(address indexed by);
    event Unpaused(address indexed by);
}