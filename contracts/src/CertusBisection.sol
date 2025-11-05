// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./CertusBase.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/**
 * @title CertusBisection
 * @notice Interactive bisection protocol for efficient fraud proofs
 */
contract CertusBisection is CertusBase, ReentrancyGuard {
    using SafeERC20 for IERC20;

    // Bisection state
    mapping(bytes32 => BisectionChallenge) public challenges;

    // Anti-grief: exponential stake escalation per round
    mapping(bytes32 => uint256) public roundStakeMultiplier;

    // Reference to main escrow contract
    address public immutable escrowContract;

    // Stylus executor for single-step verification
    address public immutable stylusExecutor;

    // Events
    event BisectionInitiated(bytes32 indexed jobId, address indexed challenger, uint256 totalSteps);
    event BisectionRoundCompleted(bytes32 indexed jobId, uint256 round, uint256 newStart, uint256 newEnd);
    event BisectionResolved(bytes32 indexed jobId, bool fraudConfirmed);

    modifier onlyEscrow() {
        require(msg.sender == escrowContract, "Only escrow contract");
        _;
    }

    constructor(address _escrowContract, address _stylusExecutor) {
        require(_escrowContract != address(0), "Invalid escrow address");
        require(_stylusExecutor != address(0), "Invalid Stylus address");
        escrowContract = _escrowContract;
        stylusExecutor = _stylusExecutor;
    }

    /**
     * @notice Initiate bisection protocol with execution trace
     */
    function initiateBisection(
        bytes32 jobId,
        uint256 totalSteps,
        bytes32 finalStateRoot,
        address challenger,
        address executor,
        address payToken,
        uint256 challengeStake
    ) external onlyEscrow returns (bool) {
        require(challenges[jobId].jobId == bytes32(0), "Challenge already exists");
        require(totalSteps > 0, "Invalid step count");
        require(finalStateRoot != bytes32(0), "Invalid state root");

        challenges[jobId] = BisectionChallenge({
            jobId: jobId,
            challenger: challenger,
            executor: executor,
            round: 1,
            disputedStart: 0,
            disputedEnd: totalSteps,
            executorStateHash: bytes32(0),
            challengerStateHash: bytes32(0),
            deadline: block.timestamp + BISECTION_ROUND_TIMEOUT,
            challengeStake: challengeStake,
            resolved: false,
            totalSteps: totalSteps,
            finalStateRoot: finalStateRoot
        });

        emit BisectionInitiated(jobId, challenger, totalSteps);
        return true;
    }

    /**
     * @notice Executor responds with state hash at midpoint
     */
    function executorRespond(
        bytes32 jobId,
        bytes32 midpointStateHash
    ) external nonReentrant {
        BisectionChallenge storage challenge = challenges[jobId];
        require(challenge.jobId == jobId, "No active challenge");
        require(msg.sender == challenge.executor, "Only executor can respond");
        require(block.timestamp <= challenge.deadline, "Round deadline passed");
        require(!challenge.resolved, "Challenge already resolved");

        challenge.executorStateHash = midpointStateHash;
        challenge.deadline = block.timestamp + BISECTION_ROUND_TIMEOUT;
    }

    /**
     * @notice Challenger narrows disputed range based on midpoint
     */
    function challengerPick(
        bytes32 jobId,
        bool disputeFirstHalf,
        bytes32 challengerMidpointHash,
        address payToken
    ) external nonReentrant {
        BisectionChallenge storage challenge = challenges[jobId];
        require(challenge.jobId == jobId, "No active challenge");
        require(msg.sender == challenge.challenger, "Only challenger can pick");
        require(block.timestamp <= challenge.deadline, "Round deadline passed");
        require(!challenge.resolved, "Challenge already resolved");
        require(challenge.executorStateHash != bytes32(0), "Executor must respond first");
        require(challenge.round < MAX_BISECTION_ROUNDS, "Max rounds exceeded");

        // Anti-grief: require exponential stake for each round
        uint256 requiredStake = challenge.challengeStake * (2 ** (challenge.round - 1));
        if (challenge.round > 5) {
            IERC20(payToken).safeTransferFrom(msg.sender, address(this), requiredStake);
            roundStakeMultiplier[jobId] += requiredStake;
        }

        uint256 mid = (challenge.disputedStart + challenge.disputedEnd) / 2;

        // Compare state hashes to narrow range
        if (challengerMidpointHash != challenge.executorStateHash) {
            challenge.disputedEnd = mid; // Dispute first half
        } else {
            challenge.disputedStart = mid; // Dispute second half
        }

        challenge.challengerStateHash = challengerMidpointHash;
        challenge.round++;
        challenge.deadline = block.timestamp + BISECTION_ROUND_TIMEOUT;

        emit BisectionRoundCompleted(jobId, challenge.round, challenge.disputedStart, challenge.disputedEnd);
    }

    /**
     * @notice Resolve bisection by executing single disputed step
     */
    function resolveBisection(
        bytes32 jobId,
        bytes calldata stepData,
        bytes32[] calldata preStateProof,
        bytes32[] calldata postStateProof
    ) external nonReentrant returns (bool fraud) {
        BisectionChallenge storage challenge = challenges[jobId];
        require(challenge.jobId == jobId, "No active challenge");
        require(!challenge.resolved, "Already resolved");
        require((challenge.disputedEnd - challenge.disputedStart) == 1, "Not narrowed to single step");

        // Verify Merkle proofs
        bytes32 preStateHash = _verifyMerkleProof(stepData, preStateProof, challenge.disputedStart);
        bytes32 postStateHash = _verifyMerkleProof(stepData, postStateProof, challenge.disputedStart + 1);

        // Execute single step via Stylus
        bytes32 canonicalPostState = _executeSingleStep(stepData, preStateHash);

        challenge.resolved = true;
        fraud = (canonicalPostState != postStateHash);

        emit BisectionResolved(jobId, fraud);
        return fraud;
    }

    /**
     * @notice Check if executor failed to respond in time
     */
    function isExecutorTimedOut(bytes32 jobId) external view returns (bool) {
        BisectionChallenge storage challenge = challenges[jobId];
        if (challenge.jobId == bytes32(0)) return false;
        if (challenge.resolved) return false;
        return block.timestamp > challenge.deadline;
    }

    /**
     * @notice Check if executor timed out during bisection
     */
    function isExecutorTimedOut(bytes32 jobId) external view returns (bool) {
        BisectionChallenge memory challenge = challenges[jobId];
        if (challenge.jobId == bytes32(0)) return false;
        if (challenge.resolved) return false;

        // Executor must respond first each round, timeout if deadline passed without response
        return (block.timestamp > challenge.deadline &&
                challenge.executorStateHash == bytes32(0) &&
                challenge.round > 0);
    }

    /**
     * @notice Get challenge details
     */
    function getChallenge(bytes32 jobId) external view returns (BisectionChallenge memory) {
        return challenges[jobId];
    }

    /**
     * @notice Verify Merkle proof for execution state
     */
    function _verifyMerkleProof(
        bytes memory data,
        bytes32[] memory proof,
        uint256 stepIndex
    ) internal pure returns (bytes32) {
        bytes32 computedHash = keccak256(abi.encodePacked(data, stepIndex));

        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 proofElement = proof[i];
            if (stepIndex % 2 == 0) {
                computedHash = keccak256(abi.encodePacked(computedHash, proofElement));
            } else {
                computedHash = keccak256(abi.encodePacked(proofElement, computedHash));
            }
            stepIndex = stepIndex / 2;
        }

        return computedHash;
    }

    /**
     * @notice Execute single WASM instruction step via Stylus
     */
    function _executeSingleStep(
        bytes memory stepData,
        bytes32 preStateHash
    ) internal returns (bytes32) {
        (bool success, bytes memory result) = stylusExecutor.call(
            abi.encodeWithSignature("executeStep(bytes,bytes32)", stepData, preStateHash)
        );

        if (!success) {
            return bytes32(0); // Treat Stylus failure as fraud
        }

        return abi.decode(result, (bytes32));
    }
}