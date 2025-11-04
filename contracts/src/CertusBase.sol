// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * @title CertusBase
 * @notice Shared data structures and constants for Certus protocol
 */
contract CertusBase {

    // Job status lifecycle
    enum Status {
        Created,    // Job created, awaiting executor
        Accepted,   // Executor accepted and posted collateral
        Receipt,    // Executor submitted output, awaiting verification
        Finalized,  // Job completed successfully
        Slashed,    // Executor slashed for fraud
        Cancelled   // Job cancelled (no executor before deadline)
    }

    // Core job structure
    struct Job {
        bytes32 jobId;
        address client;
        address executor;
        address payToken;
        uint256 payAmt;
        uint256 clientDeposit;
        uint256 executorDeposit;
        uint256 dataStorageFee;
        bytes32 wasmHash;
        bytes32 inputHash;
        bytes32 outputHash;
        bytes32 arweaveId;
        uint64 acceptDeadline;
        uint64 finalizeDeadline;
        uint64 fuelLimit;
        uint64 memLimit;
        uint32 maxOutputSize;
        Status status;
        address[3] selectedVerifiers;
        address[3] backupVerifiers;
    }

    // Verifier stake information
    struct VerifierStake {
        uint256 amount;
        address stakeToken;
        bool active;
        uint256 lastHeartbeat;
        uint256 storageCapacityGB;
        uint8 region;
        uint256 jobsVerified;
        uint256 fraudsDetected;
    }

    // Executor reputation tracking
    struct ExecutorReputation {
        uint256 jobsCompleted;
        uint256 fraudAttempts;
        uint256 lastFraudTimestamp;
        uint256 banUntil;
        bool permanentlyBanned;
    }

    // Bisection challenge for fraud proofs
    struct BisectionChallenge {
        bytes32 jobId;
        address challenger;
        address executor;
        uint256 round;
        uint256 disputedStart;
        uint256 disputedEnd;
        bytes32 executorStateHash;
        bytes32 challengerStateHash;
        uint256 deadline;
        uint256 challengeStake;
        bool resolved;
        uint256 totalSteps;
        bytes32 finalStateRoot;
    }

    // Protocol constants
    uint256 public constant MIN_VERIFIER_STAKE = 1000 * 10**6; // $1000 USDC
    uint256 public constant CHALLENGE_STAKE = 100 * 10**6; // $100 stake required
    uint256 public constant MAX_BISECTION_ROUNDS = 20;
    uint256 public constant BISECTION_ROUND_TIMEOUT = 300; // 5 minutes
    uint256 public constant MAX_GRIEF_COUNT = 3;
    uint256 public constant MAX_INPUT_ON_CHAIN = 100 * 1024; // 100KB
    uint256 public constant VERIFIER_RESPONSE_DEADLINE = 30 minutes;
    uint256 public constant HEARTBEAT_INTERVAL = 10 minutes;
    uint256 public constant MAX_REGION_CONCENTRATION = 30; // Max 30% from single region
    uint256 public constant NETWORK_PARTITION_THRESHOLD = 50; // Grace if >50% miss
    uint256 public constant VRF_FALLBACK_BLOCKS = 256;
    uint256 public constant MIN_RESPONSIVE_VERIFIERS = 2;
    uint256 public constant EXECUTOR_BAN_DURATION = 30 days;
    uint256 public constant MAX_FRAUD_ATTEMPTS = 1;
    uint256 public constant MAX_VERIFIER_SELECTION_ATTEMPTS = 200;
    uint256 public constant VERIFIER_BOUNTY_PCT = 20;
    uint256 public constant MAX_WASM_SIZE = 24 * 1024; // 24KB
    uint256 public constant VRF_RETRY_GRACE_PERIOD = 30 minutes;
    uint256 public constant EXECUTOR_COLLATERAL_MULTIPLIER = 200; // 2.0x fixed

    // Fee tier thresholds
    uint256 public constant TIER1_MAX = 10; // $10
    uint256 public constant TIER2_MAX = 100; // $100
    uint256 public constant TIER3_MAX = 1000; // $1k
    uint256 public constant TIER4_MAX = 10000; // $10k

    // Events
    event JobCreated(bytes32 indexed jobId, address indexed client, bytes32 wasmHash, uint256 payAmt);
    event JobAccepted(bytes32 indexed jobId, address indexed executor, uint256 collateral);
    event ReceiptSubmitted(bytes32 indexed jobId, bytes32 outputHash, bytes executorSig);
    event VerifiersSelected(bytes32 indexed jobId, address[3] selectedVerifiers, address[3] backupVerifiers);
    event JobFinalized(bytes32 indexed jobId, address indexed executor, uint256 payment);
    event TimeoutClaimed(bytes32 indexed jobId, address indexed executor, uint256 payment);
    event FraudDetected(bytes32 indexed jobId, address indexed executor, address verifier, uint256 slashed);
    event VerifierSlashed(bytes32 indexed jobId, address indexed verifier, address indexed reporter, uint256 penalty);
    event JobCancelled(bytes32 indexed jobId);
    event VerifierRegistered(address indexed verifier, uint256 stake, uint256 storageGB);
    event VerifierUnregistered(address indexed verifier, uint256 refundedStake);
    event ClientBanned(address indexed client, uint256 griefCount);
    event ExecutorBanned(address indexed executor, uint256 banUntil, bool permanent);
    event NetworkPartitionDetected(bytes32 indexed jobId, uint256 missedCount, uint256 totalVerifiers);
}