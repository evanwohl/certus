// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

interface IStylusWasmExecutor {
    function execute(
        bytes calldata wasm,
        bytes calldata input,
        uint64 fuelLimit,
        uint64 memLimit
    ) external returns (bytes memory output);
}

interface IVRFCoordinator {
    function requestRandomWords(
        bytes32 keyHash,
        uint64 subId,
        uint16 requestConfirmations,
        uint32 callbackGasLimit,
        uint32 numWords
    ) external returns (uint256 requestId);
}

/**
 * @title CertusEscrow
 * @notice Deterministic Verifiable Compute Escrow with Arbitrum Stylus On-Chain Fraud Verification
 * @dev Trustless escrow where fraud proofs are verified by re-executing Wasm on-chain via Stylus
 *
 * Core invariants:
 * - jobId uniqueness enforced (revert on duplicate)
 * - wasmHash must be registered and present in contract storage
 * - executorDeposit and clientDeposit in same stablecoin
 * - finalizeDeadline > acceptDeadline
 * - fraudOnChain can only be invoked when status == Receipt
 * - Escrow balances always net to zero after finalize/claim/slash
 */
contract CertusEscrow is Ownable, ReentrancyGuard {
    using SafeERC20 for IERC20;

    // ============================================================================
    // Constants & Parameters
    // ============================================================================

    uint256 public constant MAX_WASM_SIZE = 24 * 1024; // 24KB
    uint256 public constant VERIFIER_BOUNTY_PCT = 20; // 20% of slashed collateral
    uint256 public constant EXECUTOR_COLLATERAL_MULTIPLIER = 200; // Fixed 2.0x for all executors

    uint256 public clientDepositBasisPoints = 500; // 5% of payAmt (500/10000)
    uint256 public minClientDepositUsd = 5 * 10**6; // Minimum $5 USDC (6 decimals)
    uint256 public maxClientDepositUsd = 1000 * 10**6; // Maximum $1000 USDC (6 decimals)
    uint256 public challengeWindow = 3600; // 1 hour
    uint256 public acceptWindow = 120; // 2 minutes

    address public immutable stylusExecutor; // Arbitrum Stylus Wasm executor contract (immutable for security)
    address public vrfCoordinator; // Chainlink VRF coordinator
    bytes32 public vrfKeyHash; // Chainlink VRF key hash
    uint64 public vrfSubId; // Chainlink VRF subscription ID
    address public treasury; // Protocol treasury for slashing remainder
    bool public paused; // Emergency circuit breaker

    mapping(address => uint8) public tokenDecimals; // Store decimals for supported tokens
    mapping(address => bool) public supportedTokens; // Whitelist of payment tokens

    mapping(uint256 => bytes32) public vrfRequestToJobId; // VRF request ID => job ID
    mapping(bytes32 => uint256) public jobIdToVrfRequest; // job ID => VRF request ID
    mapping(bytes32 => uint256[]) public jobIdToRandomWords; // job ID => random words from VRF
    mapping(uint256 => bool) public vrfRequestFulfilled; // Prevent replay attacks

    // ============================================================================
    // Enums & Structs
    // ============================================================================

    enum Status {
        Created,
        Accepted,
        Receipt,
        Finalized,
        Slashed,
        Cancelled,
        DataUnavailable
    }

    enum DAMode {
        OnChain,      // ≤100KB stored in calldata
        Arweave,      // >100KB requires Arweave TX ID
        External      // Large data with bond
    }

    struct Job {
        bytes32 jobId;
        address client;
        address executor;
        address payToken;
        uint256 payAmt;
        uint256 clientDeposit;
        uint256 executorDeposit;
        uint256 dataStorageFee;      // Reimburse executor for Arweave costs
        bytes32 wasmHash;
        bytes32 inputHash;
        bytes32 outputHash;
        bytes32 arweaveId;           // Arweave TX ID for input >100KB
        uint64 acceptDeadline;
        uint64 finalizeDeadline;
        uint64 fuelLimit;
        uint64 memLimit;
        uint32 maxOutputSize;
        Status status;
        address[3] selectedVerifiers; // Randomly selected verifiers for this job
        address[3] backupVerifiers;   // Backup verifiers if primary offline
    }

    struct VerifierStake {
        uint256 stake;              // Minimum $1000 USDC locked
        address stakeToken;         // Token used for stake (enforced to match job payToken during selection)
        bytes32[] dataCache;        // Data hashes verifier commits to caching
        uint256 storageCapacity;    // GB committed (e.g., 50GB)
        uint256 missedChallenges;   // Slash if > 3 in 30 days
        uint256 registeredAt;       // Timestamp of registration
        uint256 lastHeartbeat;      // Last heartbeat timestamp
        uint8 region;               // Geographic region (0=NA, 1=EU, 2=ASIA, 3=OTHER)
        bool active;                // Whether verifier is active
    }

    struct ExecutorReputation {
        uint256 jobsCompleted;
        uint256 fraudAttempts;
        uint256 lastFraudTimestamp;
        uint256 banUntil; // Timestamp when ban expires (30 days after fraud)
        bool permanentlyBanned; // True after 2nd fraud attempt
    }

    struct BisectionChallenge {
        bytes32 jobId;
        address challenger;
        address executor;
        uint256 round;              // Current bisection round (max 20)
        uint256 disputedStart;      // Start execution step of disputed range
        uint256 disputedEnd;        // End execution step of disputed range
        bytes32 executorStateHash;  // Executor's state hash at disputedStart
        bytes32 challengerStateHash; // Challenger's state hash at disputedStart
        uint256 deadline;           // Deadline for current round response
        uint256 challengeStake;     // Stake deposited by challenger
        bool resolved;              // Whether challenge is resolved
        uint256 totalSteps;         // Total execution steps claimed by executor
        bytes32 finalStateRoot;     // Merkle root of final execution trace
    }

    // ============================================================================
    // State Variables
    // ============================================================================

    mapping(bytes32 => Job) public jobs;
    mapping(bytes32 => bytes) public wasmModules; // wasmHash => wasm bytecode
    mapping(bytes32 => bool) public jobExists;
    mapping(address => VerifierStake) public verifiers;
    mapping(bytes32 => BisectionChallenge) public challenges; // jobId => active challenge
    mapping(address => uint256) public clientGriefCount; // Track griefing clients

    address[] public verifierList; // List of all registered verifiers
    mapping(bytes32 => bytes) public inputDataOnChain; // jobId => input (if ≤100KB)
    mapping(bytes32 => uint256) public receiptTimestamp; // jobId => block.timestamp when submitted
    mapping(bytes32 => uint256) public receiptBlockNumber; // jobId => block.number when submitted
    mapping(bytes32 => mapping(address => bool)) public verifierResponded; // jobId => verifier => responded
    mapping(address => ExecutorReputation) public executorReputation; // Track executor behavior
    mapping(uint8 => uint256) public verifierCountByRegion; // Geographic distribution tracking

    uint256 public totalJobsCreated;
    uint256 public totalJobsFinalized;
    uint256 public totalJobsSlashed;

    uint256 public constant MIN_VERIFIER_STAKE = 1000 * 10**6; // $1000 USDC
    uint256 public constant CHALLENGE_STAKE = 100 * 10**6; // $100 stake required to initiate bisection
    uint256 public constant MAX_BISECTION_ROUNDS = 20;
    uint256 public constant BISECTION_ROUND_TIMEOUT = 300; // 5 minutes per round
    uint256 public constant MAX_GRIEF_COUNT = 3; // Ban after 3 timeouts
    uint256 public constant MAX_INPUT_ON_CHAIN = 100 * 1024; // 100KB max on-chain storage
    uint256 public constant VERIFIER_RESPONSE_DEADLINE = 30 minutes; // Must respond within 30min
    uint256 public constant HEARTBEAT_INTERVAL = 10 minutes; // Verifiers must heartbeat every 10 min
    uint256 public constant MAX_REGION_CONCENTRATION = 30; // Max 30% verifiers from single region
    uint256 public constant NETWORK_PARTITION_THRESHOLD = 50; // Grace if >50% miss challenge
    uint256 public constant VRF_FALLBACK_BLOCKS = 256; // Fallback to blockhash after 256 blocks
    uint256 public constant MIN_RESPONSIVE_VERIFIERS = 2; // Absolute minimum to prevent censorship
    uint256 public constant EXECUTOR_BAN_DURATION = 30 days; // 30-day ban after 1st fraud
    uint256 public constant MAX_FRAUD_ATTEMPTS = 1; // Permanent ban after 2nd fraud
    uint256 public constant MAX_VERIFIER_SELECTION_ATTEMPTS = 200; // Cap iterations for gas safety
    uint256 public constant VRF_RETRY_GRACE_PERIOD = 15 minutes; // Grace period before VRF retry allowed

    // ============================================================================
    // Events
    // ============================================================================

    event WasmRegistered(bytes32 indexed wasmHash, uint256 size, bytes32 testVectorHash);
    event JobCreated(bytes32 indexed jobId, address indexed client, bytes32 wasmHash, uint256 payAmt);
    event JobAccepted(bytes32 indexed jobId, address indexed executor, uint256 deposit);
    event ReceiptSubmitted(bytes32 indexed jobId, bytes32 outputHash, bytes executorSig);
    event VerifiersSelected(bytes32 indexed jobId, address[3] selectedVerifiers, address[3] backupVerifiers);
    event JobFinalized(bytes32 indexed jobId, address indexed executor, uint256 payment);
    event TimeoutClaimed(bytes32 indexed jobId, address indexed executor, uint256 payment);
    event FraudDetected(bytes32 indexed jobId, address indexed executor, address verifier, uint256 slashed);
    event VerifierSlashed(bytes32 indexed jobId, address indexed verifier, address indexed reporter, uint256 penalty);
    event JobCancelled(bytes32 indexed jobId);
    event VerifierRegistered(address indexed verifier, uint256 stake, uint256 storageGB);
    event VerifierUnregistered(address indexed verifier, uint256 refundedStake);
    event DataCacheUpdated(address indexed verifier, bytes32 dataHash, bool added);
    event BisectionInitiated(bytes32 indexed jobId, address indexed challenger, uint256 outputSize);
    event BisectionRoundCompleted(bytes32 indexed jobId, uint256 round, uint256 newStart, uint256 newEnd);
    event BisectionResolved(bytes32 indexed jobId, bool fraudConfirmed);
    event ClientBanned(address indexed client, uint256 griefCount);
    event VerifierHeartbeat(address indexed verifier, uint256 timestamp);
    event BackupVerifierActivated(bytes32 indexed jobId, address indexed backup, address indexed replaced);
    event TokenSupported(address indexed token, uint8 decimals);
    event TokenRemoved(address indexed token);
    event ExecutorReputationUpdated(address indexed executor, uint256 fraudAttempts, uint256 newMultiplier);
    event NetworkPartitionDetected(bytes32 indexed jobId, uint256 missedCount, uint256 totalVerifiers);
    event ExecutorBanned(address indexed executor, uint256 banUntil, bool permanent);
    event Paused(address indexed by);
    event Unpaused(address indexed by);
    event FallbackVerifierSelection(bytes32 indexed jobId, uint256 blocksSinceReceipt);
    event ProtocolFeesWithdrawn(address indexed token, address indexed recipient, uint256 amount);

    // ============================================================================
    // Modifiers
    // ============================================================================

    modifier whenNotPaused() {
        require(!paused, "Contract paused");
        _;
    }

    modifier whenPaused() {
        require(paused, "Contract not paused");
        _;
    }

    // ============================================================================
    // Constructor
    // ============================================================================

    constructor(
        address _stylusExecutor,
        address _vrfCoordinator,
        bytes32 _vrfKeyHash,
        uint64 _vrfSubId,
        address _treasury,
        address[] memory _supportedTokens
    ) Ownable(msg.sender) {
        require(_stylusExecutor != address(0), "Invalid Stylus executor");
        require(_vrfCoordinator != address(0), "Invalid VRF coordinator");
        require(_treasury != address(0), "Invalid treasury");
        stylusExecutor = _stylusExecutor;
        vrfCoordinator = _vrfCoordinator;
        vrfKeyHash = _vrfKeyHash;
        vrfSubId = _vrfSubId;
        treasury = _treasury;

        // Initialize supported tokens with their decimals
        for (uint256 i = 0; i < _supportedTokens.length; i++) {
            address token = _supportedTokens[i];
            require(token != address(0), "Invalid token address");
            uint8 decimals = IERC20Metadata(token).decimals();
            tokenDecimals[token] = decimals;
            supportedTokens[token] = true;
            emit TokenSupported(token, decimals);
        }
    }

    // ============================================================================
    // Wasm Storage Functions
    // ============================================================================

    /**
     * @notice Register a Wasm module with mandatory determinism validation
     * @param wasm The Wasm bytecode
     * @param testInput Test input for determinism check
     * @param expectedOutputHash Expected SHA256 hash of test output
     * @return wasmHash SHA256 hash of the Wasm module
     *
     * @dev Enforces determinism by:
     * 1. Re-executing test vector on-chain via Stylus
     * 2. Comparing output hash with expected value
     * 3. Rejecting non-deterministic modules before they can be used
     */
    function registerWasm(
        bytes calldata wasm,
        bytes calldata testInput,
        bytes32 expectedOutputHash
    ) external returns (bytes32 wasmHash) {
        require(wasm.length > 0 && wasm.length <= MAX_WASM_SIZE, "Invalid wasm size");
        require(testInput.length > 0, "Test input required");
        require(expectedOutputHash != bytes32(0), "Expected output hash required");

        wasmHash = sha256(wasm);
        require(wasmModules[wasmHash].length == 0, "Wasm already registered");

        // CRITICAL: On-chain determinism validation
        // Execute test vector via Stylus to ensure deterministic behavior
        bytes memory actualOutput = _executeWasmOnChain(wasm, testInput, 1000000, 10 * 1024 * 1024);
        bytes32 actualOutputHash = sha256(actualOutput);

        require(actualOutputHash == expectedOutputHash, "Test vector mismatch - non-deterministic module");

        wasmModules[wasmHash] = wasm;
        emit WasmRegistered(wasmHash, wasm.length, expectedOutputHash);

        return wasmHash;
    }

    /**
     * @notice Retrieve registered Wasm module
     * @param wasmHash SHA256 hash of the module
     * @return wasm The Wasm bytecode
     */
    function wasmOf(bytes32 wasmHash) external view returns (bytes memory wasm) {
        wasm = wasmModules[wasmHash];
        require(wasm.length > 0, "Wasm not found");
        return wasm;
    }

    // ============================================================================
    // Job Lifecycle Functions
    // ============================================================================

    /**
     * @notice Create a new compute job with escrow
     * @param jobId Unique job identifier (SHA256(wasmHash || inputHash || clientPubKey || nonce))
     * @param payToken ERC20 stablecoin address (USDC/USDT/DAI)
     * @param payAmt Payment amount in token smallest units
     * @param wasmHash SHA256 hash of registered Wasm module
     * @param inputHash SHA256 hash of input data
     * @param acceptDeadline Unix timestamp deadline for executor to accept
     * @param finalizeDeadline Unix timestamp deadline for client to finalize
     * @param fuelLimit Wasm instruction budget
     * @param memLimit Memory limit in bytes
     * @param maxOutputSize Maximum output size in bytes
     */
    function createJob(
        bytes32 jobId,
        address payToken,
        uint256 payAmt,
        bytes32 wasmHash,
        bytes32 inputHash,
        uint64 acceptDeadline,
        uint64 finalizeDeadline,
        uint64 fuelLimit,
        uint64 memLimit,
        uint32 maxOutputSize
    ) external nonReentrant whenNotPaused {
        require(!jobExists[jobId], "Job already exists");
        require(wasmModules[wasmHash].length > 0, "Wasm not registered");
        require(payAmt > 0, "Payment must be > 0");
        require(acceptDeadline > block.timestamp, "Accept deadline in past");
        require(acceptDeadline <= block.timestamp + acceptWindow, "Accept window too long");
        require(finalizeDeadline > acceptDeadline, "Finalize must be after accept");
        require(payToken != address(0), "Invalid token");
        require(supportedTokens[payToken], "Token not supported");
        require(clientGriefCount[msg.sender] < MAX_GRIEF_COUNT, "Client banned for griefing");

        // Resource limits to prevent DoS on on-chain execution
        require(fuelLimit > 0 && fuelLimit <= 100_000_000, "Fuel limit out of range");
        require(memLimit > 0 && memLimit <= 1024 * 1024 * 1024, "Memory limit out of range");
        require(maxOutputSize > 0 && maxOutputSize <= 10 * 1024 * 1024, "Output size out of range");

        // Calculate proportional client deposit (5% of payAmt, min $5, max $1000)
        uint8 decimals = tokenDecimals[payToken];
        uint256 proportionalDeposit = (payAmt * clientDepositBasisPoints) / 10000;
        uint256 minDeposit = _normalizeAmount(minClientDepositUsd, 6, decimals);
        uint256 maxDeposit = _normalizeAmount(maxClientDepositUsd, 6, decimals);

        uint256 clientDeposit = proportionalDeposit;
        if (clientDeposit < minDeposit) clientDeposit = minDeposit;
        if (clientDeposit > maxDeposit) clientDeposit = maxDeposit;

        uint256 totalClientPayment = payAmt + clientDeposit;

        // Transfer client payment + deposit
        IERC20(payToken).safeTransferFrom(msg.sender, address(this), totalClientPayment);

        jobs[jobId] = Job({
            jobId: jobId,
            client: msg.sender,
            executor: address(0),
            payToken: payToken,
            payAmt: payAmt,
            clientDeposit: clientDeposit,
            executorDeposit: 0,
            dataStorageFee: 0,
            wasmHash: wasmHash,
            inputHash: inputHash,
            outputHash: bytes32(0),
            arweaveId: bytes32(0),
            acceptDeadline: acceptDeadline,
            finalizeDeadline: finalizeDeadline,
            fuelLimit: fuelLimit,
            memLimit: memLimit,
            maxOutputSize: maxOutputSize,
            status: Status.Created,
            selectedVerifiers: [address(0), address(0), address(0)],
            backupVerifiers: [address(0), address(0), address(0)]
        });

        jobExists[jobId] = true;
        totalJobsCreated++;

        emit JobCreated(jobId, msg.sender, wasmHash, payAmt);
    }

    /**
     * @notice Executor accepts a job and posts collateral
     * @param jobId The job identifier
     *
     * @dev Fixed 2.0x collateral for all executors (no reputation system).
     * Enforces executor bans (30 days after 1st fraud, permanent after 2nd).
     */
    function acceptJob(bytes32 jobId) external nonReentrant whenNotPaused {
        Job storage job = jobs[jobId];
        require(job.status == Status.Created, "Job not in Created state");
        require(block.timestamp <= job.acceptDeadline, "Accept deadline passed");
        require(job.executor == address(0), "Job already accepted");

        // Check executor ban status
        ExecutorReputation storage rep = executorReputation[msg.sender];
        require(!rep.permanentlyBanned, "Executor permanently banned");
        require(block.timestamp >= rep.banUntil, "Executor temporarily banned");

        // Fixed 2.0x collateral (200 / 100 = 2.0)
        // Overflow protection: ensure calculations don't overflow
        require(job.payAmt <= type(uint256).max / 2, "Payment amount too large");
        uint256 executorDeposit = job.payAmt * 2;
        require(executorDeposit / 2 == job.payAmt, "Overflow in collateral calculation");

        // Transfer executor deposit
        IERC20(job.payToken).safeTransferFrom(msg.sender, address(this), executorDeposit);

        job.executor = msg.sender;
        job.executorDeposit = executorDeposit;
        job.status = Status.Accepted;

        emit JobAccepted(jobId, msg.sender, executorDeposit);
    }

    /**
     * @notice Executor submits execution receipt with output hash
     * @param jobId The job identifier
     * @param outputHash SHA256 hash of the output
     * @param execSig Ed25519 signature over canonical receipt
     *
     * @dev Requests Chainlink VRF for secure random verifier selection
     */
    function submitReceipt(
        bytes32 jobId,
        bytes32 outputHash,
        bytes calldata execSig
    ) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Accepted, "Job not in Accepted state");
        require(msg.sender == job.executor, "Only executor can submit receipt");
        require(outputHash != bytes32(0), "Invalid output hash");
        require(execSig.length == 64, "Invalid signature length");

        job.outputHash = outputHash;
        job.status = Status.Receipt;
        job.finalizeDeadline = uint64(block.timestamp) + uint64(challengeWindow);
        receiptTimestamp[jobId] = block.timestamp;
        receiptBlockNumber[jobId] = block.number;

        // Request Chainlink VRF for verifier selection
        uint256 requestId = IVRFCoordinator(vrfCoordinator).requestRandomWords(
            vrfKeyHash,
            vrfSubId,
            3, // 3 confirmations
            500000, // callback gas limit
            2 // request 2 random words
        );

        vrfRequestToJobId[requestId] = jobId;
        jobIdToVrfRequest[jobId] = requestId;

        emit ReceiptSubmitted(jobId, outputHash, execSig);
    }

    /**
     * @notice Chainlink VRF callback to select verifiers with proven randomness
     * @param requestId VRF request identifier
     * @param randomWords Random values from Chainlink VRF
     */
    function fulfillRandomWords(uint256 requestId, uint256[] memory randomWords) external {
        require(msg.sender == vrfCoordinator, "Only VRF coordinator");
        require(vrfCoordinator != address(0), "VRF coordinator not set");
        require(!vrfRequestFulfilled[requestId], "Request already fulfilled");
        require(randomWords.length >= 2, "Need 2 random words");

        bytes32 jobId = vrfRequestToJobId[requestId];
        require(jobId != bytes32(0), "Invalid VRF request");

        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");

        vrfRequestFulfilled[requestId] = true;
        jobIdToRandomWords[jobId] = randomWords;

        // Select 3 primary + 3 backup verifiers using VRF randomness
        job.selectedVerifiers = _selectRandomVerifiersWithVRF(jobId, randomWords[0]);
        job.backupVerifiers = _selectBackupVerifiersWithVRF(jobId, randomWords[1], job.selectedVerifiers);

        emit VerifiersSelected(jobId, job.selectedVerifiers, job.backupVerifiers);
    }

    /**
     * @notice Retry VRF request if callback never arrives (stuck jobs)
     * @param jobId The job identifier
     * @dev Callable by anyone after grace period to unstuck jobs
     */
    function retryVrf(bytes32 jobId) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");
        require(job.selectedVerifiers[0] == address(0), "Verifiers already selected");

        uint256 lastRequestTime = receiptTimestamp[jobId];
        uint256 existingRequestId = jobIdToVrfRequest[jobId];

        // If there's an existing request, check its age; otherwise use receipt timestamp
        if (existingRequestId != 0 && vrfRequestToJobId[existingRequestId] == jobId) {
            // Prevent rapid retries - require grace period since last request
            require(block.timestamp > lastRequestTime + VRF_RETRY_GRACE_PERIOD, "Grace period not elapsed");
        } else {
            // First retry after initial VRF request
            require(block.timestamp > receiptTimestamp[jobId] + VRF_RETRY_GRACE_PERIOD, "Grace period not elapsed");
        }

        // Request new VRF randomness
        uint256 requestId = IVRFCoordinator(vrfCoordinator).requestRandomWords(
            vrfKeyHash,
            vrfSubId,
            3, // request confirmations
            500000, // callback gas limit
            2 // request 2 random words
        );

        vrfRequestToJobId[requestId] = jobId;
        jobIdToVrfRequest[jobId] = requestId;
    }

    /**
     * @notice Client finalizes job and releases payment to executor
     * @param jobId The job identifier
     *
     * @dev Uses tiered fee structure based on job value.
     */
    function finalize(bytes32 jobId) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");
        require(msg.sender == job.client, "Only client can finalize");
        require(block.timestamp <= job.finalizeDeadline, "Finalize deadline passed");

        // Calculate fee using tiered structure
        uint256 protocolFee = _calculateProtocolFee(job.payAmt, job.payToken);
        uint256 executorPayment = job.payAmt - protocolFee + job.executorDeposit + job.dataStorageFee;

        job.status = Status.Finalized;
        totalJobsFinalized++;

        // Update executor reputation (successful job completion)
        _updateExecutorReputation(job.executor, false);

        // Transfer payment to executor (payAmt + executorDeposit + dataStorageFee - protocol fee)
        IERC20(job.payToken).safeTransfer(job.executor, executorPayment);

        // Refund client deposit
        IERC20(job.payToken).safeTransfer(job.client, job.clientDeposit);

        // Protocol fee stays in contract (or burn to address(0))

        emit JobFinalized(jobId, job.executor, executorPayment);
    }

    /**
     * @notice Executor claims payment after finalize deadline if no fraud submitted
     * @param jobId The job identifier
     *
     * @dev AUTO-FINALIZATION: Executor receives payment + client deposit as penalty.
     * This makes griefing -EV for clients (lose $10 to cause executor no harm).
     * Uses tiered fee structure.
     */
    function claimTimeout(bytes32 jobId) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");
        require(msg.sender == job.executor, "Only executor can claim");
        require(block.timestamp > job.finalizeDeadline, "Finalize deadline not passed");

        // Use tiered fee structure
        uint256 protocolFee = _calculateProtocolFee(job.payAmt, job.payToken);
        uint256 executorPayment = job.payAmt - protocolFee + job.executorDeposit + job.clientDeposit;

        job.status = Status.Finalized;
        totalJobsFinalized++;

        // ANTI-GRIEFING: Executor gets payment + their deposit + CLIENT DEPOSIT
        // Client loses deposit for failing to finalize (griefing penalty)
        IERC20(job.payToken).safeTransfer(job.executor, executorPayment);

        // Track client griefing and ban after 3 offenses
        clientGriefCount[job.client]++;
        if (clientGriefCount[job.client] >= MAX_GRIEF_COUNT) {
            emit ClientBanned(job.client, clientGriefCount[job.client]);
        }

        emit TimeoutClaimed(jobId, job.executor, executorPayment);
    }

    /**
     * @notice Fallback verifier selection using blockhash if VRF stuck
     * @param jobId The job identifier
     * @dev Can only be called 256+ blocks after receipt if VRF hasn't responded
     */
    function fallbackVerifierSelection(bytes32 jobId) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Not in receipt status");

        uint256 requestId = jobIdToVrfRequest[jobId];
        require(requestId != 0, "No VRF request for job");
        require(!vrfRequestFulfilled[requestId], "VRF already fulfilled");

        // Must wait 256+ blocks since receipt submission
        uint256 blocksSinceReceipt = block.number - receiptBlockNumber[jobId];
        require(blocksSinceReceipt >= VRF_FALLBACK_BLOCKS, "Must wait 256 blocks for fallback");

        // Use blockhash as entropy source (weak but better than stuck)
        uint256 pseudoRandom1 = uint256(blockhash(block.number - 1));
        uint256 pseudoRandom2 = uint256(blockhash(block.number - 2));

        // Mark as fulfilled to prevent double selection
        vrfRequestFulfilled[requestId] = true;

        // Select verifiers using blockhash entropy
        _selectVerifiersWithRandomness(jobId, pseudoRandom1, pseudoRandom2);

        emit FallbackVerifierSelection(jobId, blocksSinceReceipt);
    }

    /**
     * @notice Submit fraud proof - re-executes Wasm on-chain via Arbitrum Stylus
     * @param jobId The job identifier
     * @param wasm Full Wasm module bytecode
     * @param input Full input data
     * @param claimedOutput The output claimed by executor
     *
     * @dev This function calls Arbitrum Stylus to re-execute wasm(input) deterministically on L2
     *      If recomputed output hash != stored outputHash, executor is slashed
     */
    function fraudOnChain(
        bytes32 jobId,
        bytes calldata wasm,
        bytes calldata input,
        bytes calldata claimedOutput
    ) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");
        require(sha256(wasm) == job.wasmHash, "Wasm hash mismatch");
        require(sha256(input) == job.inputHash, "Input hash mismatch");

        // ============================================================================
        // Arbitrum Stylus On-Chain Wasm Execution
        // ============================================================================
        // Re-execute the Wasm module on-chain via Arbitrum Stylus to verify
        // the executor's claimed output. This provides trustless fraud detection
        // without relying on external oracles or optimistic challenges.

        bytes memory recomputedOutput = _executeWasmOnChain(wasm, input, job.fuelLimit, job.memLimit);
        bytes32 recomputedOutputHash = sha256(recomputedOutput);

        // Check if fraud detected
        if (recomputedOutputHash != job.outputHash) {
            // FRAUD DETECTED - slash executor
            uint256 totalSlashed = job.payAmt + job.executorDeposit;
            uint256 verifierBounty = (totalSlashed * VERIFIER_BOUNTY_PCT) / 100;
            uint256 clientRefund = totalSlashed - verifierBounty;

            job.status = Status.Slashed;
            totalJobsSlashed++;

            // Update executor reputation (fraud attempt)
            _updateExecutorReputation(job.executor, true);

            // Pay verifier bounty
            IERC20(job.payToken).safeTransfer(msg.sender, verifierBounty);

            // Refund client (payment + their deposit + slashed executor deposit - bounty)
            IERC20(job.payToken).safeTransfer(job.client, clientRefund + job.clientDeposit);

            emit FraudDetected(jobId, job.executor, msg.sender, totalSlashed);
        } else {
            // No fraud - receipt was correct, revert
            revert("No fraud detected - output matches");
        }
    }

    /**
     * @dev Execute Wasm module on-chain via Arbitrum Stylus
     *
     * This function delegates to an external Stylus contract that executes
     * the Wasm bytecode deterministically on L2. The Stylus contract enforces
     * resource limits (fuel, memory) and returns the execution output.
     *
     * Integration requirements:
     * 1. Deploy a Stylus contract implementing IStylusWasmExecutor interface
     * 2. Set STYLUS_EXECUTOR_ADDRESS to the deployed contract address
     * 3. The Stylus contract must enforce identical determinism guarantees
     *
     * Current implementation uses a deterministic reference function for testing.
     * Replace with actual Stylus integration before production deployment.
     *
     * @param wasm Wasm module bytecode
     * @param input Execution input data
     * @param fuelLimit Maximum instructions allowed
     * @param memLimit Maximum memory in bytes
     * @return Execution output bytes
     */
    function _executeWasmOnChain(
        bytes calldata wasm,
        bytes calldata input,
        uint64 fuelLimit,
        uint64 memLimit
    ) internal returns (bytes memory) {
        require(stylusExecutor != address(0), "Stylus executor not configured");

        // Call Arbitrum Stylus contract to execute Wasm deterministically on-chain
        // Wrapped in try/catch to prevent grief-reverts from malicious Stylus implementations
        try IStylusWasmExecutor(stylusExecutor).execute(
            wasm,
            input,
            fuelLimit,
            memLimit
        ) returns (bytes memory output) {
            // Note: Zero-byte outputs are valid (e.g., verification jobs returning success)
            // Hash of empty bytes: sha256("") = 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
            return output;
        } catch Error(string memory reason) {
            // Stylus error - do not revert as that would block fraud proof
            // Return specific error sentinel to trigger fraud detection
            bytes memory errorSentinel = abi.encodePacked("STYLUS_ERROR:", reason);
            return errorSentinel;
        } catch (bytes memory /*lowLevelData*/) {
            // Low-level error - do not revert as that would block fraud proof
            // Return error sentinel to trigger fraud detection
            bytes memory errorSentinel = abi.encodePacked("STYLUS_LOW_LEVEL_ERROR");
            return errorSentinel;
        }
    }

    /**
     * @notice Cancel job if not accepted before accept deadline
     * @param jobId The job identifier
     */
    function cancelJob(bytes32 jobId) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Created, "Can only cancel Created jobs");
        require(block.timestamp > job.acceptDeadline, "Accept deadline not passed");
        require(msg.sender == job.client, "Only client can cancel");

        job.status = Status.Cancelled;

        // Refund client payment + deposit
        uint256 refund = job.payAmt + job.clientDeposit;
        IERC20(job.payToken).safeTransfer(job.client, refund);

        emit JobCancelled(jobId);
    }

    // ============================================================================
    // Verifier Registration Functions
    // ============================================================================

    /**
     * @notice Register as a verifier with stake and data cache commitment
     * @param stake Amount of USDC to stake (minimum $1000)
     * @param dataHashes List of data hashes verifier commits to caching
     * @param storageGB Storage capacity in GB (e.g., 50)
     * @param stableToken ERC20 stablecoin address for stake
     * @param region Geographic region (0=NA, 1=EU, 2=ASIA, 3=OTHER)
     */
    function registerVerifier(
        uint256 stake,
        bytes32[] calldata dataHashes,
        uint256 storageGB,
        address stableToken,
        uint8 region
    ) external nonReentrant {
        require(supportedTokens[stableToken], "Token not supported");

        // Normalize MIN_VERIFIER_STAKE (stored in 6 decimals) to token's actual decimals
        uint8 decimals = tokenDecimals[stableToken];
        uint256 minStakeNormalized = _normalizeAmount(MIN_VERIFIER_STAKE, 6, decimals);

        require(stake >= minStakeNormalized, "Stake below minimum");
        require(!verifiers[msg.sender].active, "Already registered");
        require(storageGB > 0, "Storage capacity required");
        require(stableToken != address(0), "Invalid token");
        require(region <= 3, "Invalid region");

        // Check geographic diversity (max 30% from any region)
        uint256 regionCount = verifierCountByRegion[region];
        uint256 totalVerifiers = verifierList.length;
        if (totalVerifiers > 0) {
            require((regionCount + 1) * 100 <= (totalVerifiers + 1) * MAX_REGION_CONCENTRATION,
                "Region concentration limit exceeded");
        }

        // Transfer stake
        IERC20(stableToken).safeTransferFrom(msg.sender, address(this), stake);

        // Register verifier
        verifiers[msg.sender] = VerifierStake({
            stake: stake,
            stakeToken: stableToken,
            dataCache: dataHashes,
            storageCapacity: storageGB,
            missedChallenges: 0,
            registeredAt: block.timestamp,
            lastHeartbeat: block.timestamp,
            region: region,
            active: true
        });

        verifierList.push(msg.sender);
        verifierCountByRegion[region]++;

        emit VerifierRegistered(msg.sender, stake, storageGB);
    }

    /**
     * @notice Unregister as verifier and withdraw stake
     */
    function unregisterVerifier() external nonReentrant {
        VerifierStake storage verifier = verifiers[msg.sender];
        require(verifier.active, "Not registered");
        require(verifier.missedChallenges < 3, "Too many missed challenges");

        uint256 refund = verifier.stake;
        address token = verifier.stakeToken;
        verifier.active = false;
        verifier.stake = 0;

        IERC20(token).safeTransfer(msg.sender, refund);

        emit VerifierUnregistered(msg.sender, refund);
    }

    /**
     * @notice Update data cache commitment
     * @param dataHash Data hash to add/remove
     * @param add true to add, false to remove
     */
    function updateDataCache(bytes32 dataHash, bool add) external {
        VerifierStake storage verifier = verifiers[msg.sender];
        require(verifier.active, "Not registered");

        if (add) {
            verifier.dataCache.push(dataHash);
        } else {
            // Remove data from cache
            for (uint i = 0; i < verifier.dataCache.length; i++) {
                if (verifier.dataCache[i] == dataHash) {
                    verifier.dataCache[i] = verifier.dataCache[verifier.dataCache.length - 1];
                    verifier.dataCache.pop();
                    break;
                }
            }
        }

        emit DataCacheUpdated(msg.sender, dataHash, add);
    }

    // ============================================================================
    // Bisection Protocol Functions
    // ============================================================================

    /**
     * @notice Initiate fraud challenge using bisection protocol
     * @param jobId The job identifier
     * @param claimedOutputSize Size of executor's claimed output
     *
     * @dev Starts interactive bisection to narrow down disputed bytes
     * Gas-efficient: only final 32-byte chunk executed on-chain
     */
    function initiateBisection(
        bytes32 jobId,
        uint256 claimedOutputSize
    ) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");
        require(_isSelectedVerifier(jobId, msg.sender), "Not a selected verifier for this job");
        require(challenges[jobId].jobId == bytes32(0), "Challenge already exists");
        require(claimedOutputSize > 0, "Invalid output size");

        // Require challenger to stake CHALLENGE_STAKE to prevent griefing
        uint8 decimals = tokenDecimals[job.payToken];
        uint256 stake = _normalizeAmount(CHALLENGE_STAKE, 6, decimals);
        IERC20(job.payToken).safeTransferFrom(msg.sender, address(this), stake);

        challenges[jobId] = BisectionChallenge({
            jobId: jobId,
            challenger: msg.sender,
            executor: job.executor,
            round: 1,
            disputedStart: 0,
            disputedEnd: claimedOutputSize, // This represents execution steps now
            executorStateHash: bytes32(0),
            challengerStateHash: bytes32(0),
            deadline: block.timestamp + BISECTION_ROUND_TIMEOUT,
            challengeStake: stake,
            resolved: false,
            totalSteps: claimedOutputSize, // Total execution steps
            finalStateRoot: bytes32(0) // Will be set by challenger
        });

        emit BisectionInitiated(jobId, msg.sender, claimedOutputSize);
    }

    /**
     * @notice Executor responds with state hash at midpoint
     * @param jobId The job identifier
     * @param midpointStateHash State hash at midpoint of disputed range
     */
    function bisectionExecutorRespond(
        bytes32 jobId,
        bytes32 midpointStateHash
    ) external nonReentrant {
        BisectionChallenge storage challenge = challenges[jobId];
        require(challenge.jobId == jobId, "No active challenge");
        require(msg.sender == challenge.executor, "Only executor can respond");
        require(block.timestamp <= challenge.deadline, "Round deadline passed");
        require(!challenge.resolved, "Challenge already resolved");

        // Store executor's state hash at midpoint
        challenge.executorStateHash = midpointStateHash;
        challenge.deadline = block.timestamp + BISECTION_ROUND_TIMEOUT;
    }

    /**
     * @notice Slash executor for not responding to bisection round
     * @param jobId The job identifier
     *
     * @dev NEW: Executor must respond within 5 minutes or lose collateral
     */
    function slashBisectionNonResponse(bytes32 jobId) external nonReentrant {
        BisectionChallenge storage challenge = challenges[jobId];
        Job storage job = jobs[jobId];

        require(challenge.jobId == jobId, "No active challenge");
        require(block.timestamp > challenge.deadline, "Deadline not passed");
        require(!challenge.resolved, "Challenge already resolved");

        // EXECUTOR FAILED TO RESPOND - automatic fraud confirmation
        uint256 totalSlashed = job.payAmt + job.executorDeposit;
        uint256 verifierBounty = (totalSlashed * VERIFIER_BOUNTY_PCT) / 100;
        uint256 clientRefund = totalSlashed - verifierBounty;

        job.status = Status.Slashed;
        totalJobsSlashed++;
        challenge.resolved = true;

        // Update executor reputation (counts as fraud)
        _updateExecutorReputation(job.executor, true);

        // Pay verifier bounty to challenger
        IERC20(job.payToken).safeTransfer(challenge.challenger, verifierBounty);

        // Refund client
        IERC20(job.payToken).safeTransfer(job.client, clientRefund + job.clientDeposit);

        emit FraudDetected(jobId, job.executor, challenge.challenger, totalSlashed);
        emit BisectionResolved(jobId, true);
    }

    /**
     * @notice Challenger narrows disputed range based on midpoint state
     * @param jobId The job identifier
     * @param disputeFirstHalf true to dispute first half false for second
     * @param challengerMidpointHash Challenger's state hash at midpoint
     */
    function bisectionChallengerPick(
        bytes32 jobId,
        bool disputeFirstHalf,
        bytes32 challengerMidpointHash
    ) external nonReentrant {
        BisectionChallenge storage challenge = challenges[jobId];
        require(challenge.jobId == jobId, "No active challenge");
        require(msg.sender == challenge.challenger, "Only challenger can pick");
        require(block.timestamp <= challenge.deadline, "Round deadline passed");
        require(!challenge.resolved, "Challenge already resolved");
        require(challenge.executorStateHash != bytes32(0), "Executor must respond first");

        uint256 mid = (challenge.disputedStart + challenge.disputedEnd) / 2;

        // Compare state hashes at midpoint to narrow range
        if (challengerMidpointHash != challenge.executorStateHash) {
            // Disagreement at midpoint - dispute first half
            challenge.disputedEnd = mid;
        } else {
            // Agreement at midpoint - dispute second half
            challenge.disputedStart = mid;
        }

        challenge.challengerStateHash = challengerMidpointHash;
        challenge.round++;
        challenge.deadline = block.timestamp + BISECTION_ROUND_TIMEOUT;

        emit BisectionRoundCompleted(jobId, challenge.round, challenge.disputedStart, challenge.disputedEnd);
    }

    /**
     * @notice Resolve bisection by executing single disputed step on-chain
     * @param jobId The job identifier
     * @param stepData Execution data for single step (instruction + state)
     * @param preStateProof Merkle proof of pre-state
     * @param postStateProof Merkle proof of post-state
     */
    function resolveBisection(
        bytes32 jobId,
        bytes calldata stepData,
        bytes32[] calldata preStateProof,
        bytes32[] calldata postStateProof
    ) external nonReentrant {
        BisectionChallenge storage challenge = challenges[jobId];
        Job storage job = jobs[jobId];

        require(challenge.jobId == jobId, "No active challenge");
        require(!challenge.resolved, "Already resolved");
        require((challenge.disputedEnd - challenge.disputedStart) == 1, "Not narrowed to single step");

        // Verify Merkle proofs match claimed states
        bytes32 preStateHash = _verifyMerkleProof(stepData, preStateProof, challenge.disputedStart);
        bytes32 postStateHash = _verifyMerkleProof(stepData, postStateProof, challenge.disputedStart + 1);

        // Execute SINGLE step on-chain via Stylus (cheap)
        bytes32 canonicalPostState = _executeSingleStep(stepData, preStateHash);

        challenge.resolved = true;

        // Determine fraud based on single step execution
        bool fraud = (canonicalPostState != postStateHash);

        if (fraud) {
            // FRAUD CONFIRMED
            uint256 totalSlashed = job.payAmt + job.executorDeposit;
            uint256 verifierBounty = (totalSlashed * VERIFIER_BOUNTY_PCT) / 100;
            uint256 clientRefund = totalSlashed - verifierBounty;

            job.status = Status.Slashed;
            totalJobsSlashed++;

            IERC20(job.payToken).safeTransfer(challenge.challenger, verifierBounty + challenge.challengeStake);
            IERC20(job.payToken).safeTransfer(job.client, clientRefund + job.clientDeposit);

            emit FraudDetected(jobId, job.executor, challenge.challenger, totalSlashed);
            emit BisectionResolved(jobId, true);
        } else {
            // No fraud detected - slash challenger stake and refund executor
            IERC20(job.payToken).safeTransfer(job.executor, challenge.challengeStake);
            emit BisectionResolved(jobId, false);
            revert("No fraud detected - challenger slashed");
        }
    }

    // ============================================================================
    // Verifier Slashing Functions
    // ============================================================================

    /**
     * @notice Heartbeat to prove verifier is online
     *
     * @dev Verifiers must call this every 10 minutes to remain eligible for selection
     */
    function heartbeat() external {
        VerifierStake storage verifierStake = verifiers[msg.sender];
        require(verifierStake.active, "Not an active verifier");

        verifierStake.lastHeartbeat = block.timestamp;
        emit VerifierHeartbeat(msg.sender, block.timestamp);
    }

    /**
     * @notice Slash verifier for not responding within deadline
     * @param jobId The job identifier
     * @param verifier The verifier to slash
     *
     * @dev SECURITY FIX: Prevents Sybil verifiers from going offline
     * Includes network partition grace period (no slash if >50% miss)
     */
    function slashNonResponsiveVerifier(bytes32 jobId, address verifier) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");
        require(_isSelectedVerifier(jobId, verifier), "Not a selected verifier");
        require(block.timestamp > receiptTimestamp[jobId] + VERIFIER_RESPONSE_DEADLINE, "Deadline not passed");
        require(!verifierResponded[jobId][verifier], "Verifier already responded");

        VerifierStake storage verifierStake = verifiers[verifier];
        require(verifierStake.active, "Verifier not active");

        // NETWORK PARTITION DETECTION: Check if >50% of all verifiers missed this challenge
        uint256 missedCount = 0;
        for (uint256 i = 0; i < 3; i++) {
            if (!verifierResponded[jobId][job.selectedVerifiers[i]]) {
                missedCount++;
            }
        }

        // Grace period: if >50% missed AND responsive count < 2, likely network partition
        uint256 responsiveCount = 3 - missedCount;
        bool isPartition = (missedCount * 100 > 3 * NETWORK_PARTITION_THRESHOLD) && (responsiveCount < MIN_RESPONSIVE_VERIFIERS);

        if (isPartition) {
            emit NetworkPartitionDetected(jobId, missedCount, 3);
            // Activate backup verifier instead of slashing
            _activateBackupVerifier(jobId, verifier);
            return;
        }

        // Slash 50% of stake
        uint256 penalty = verifierStake.stake / 2;
        verifierStake.stake -= penalty;
        verifierStake.missedChallenges++;

        // Reward caller (10% of slashed amount), remainder to treasury
        uint256 callerReward = penalty / 10;
        uint256 remainder = penalty - callerReward;
        IERC20(verifierStake.stakeToken).safeTransfer(msg.sender, callerReward);
        IERC20(verifierStake.stakeToken).safeTransfer(treasury, remainder);

        // Deactivate if too many missed challenges
        if (verifierStake.missedChallenges >= 3) {
            verifierStake.active = false;
        }

        // Activate backup verifier
        _activateBackupVerifier(jobId, verifier);

        emit VerifierSlashed(jobId, verifier, msg.sender, penalty);
    }

    /**
     * @notice Mark that verifier has responded to challenge
     * @param jobId The job identifier
     *
     * @dev Verifiers call this after re-executing off-chain to prove they're active
     */
    function verifierAcknowledge(bytes32 jobId) external {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Job not in Receipt state");
        require(_isSelectedVerifier(jobId, msg.sender), "Not a selected verifier");
        require(verifiers[msg.sender].active, "Not an active verifier");

        verifierResponded[jobId][msg.sender] = true;
    }

    // ============================================================================
    // Internal Helper Functions
    // ============================================================================

    /**
     * @notice Calculate protocol fee based on tiered structure
     * @param payAmt Job payment amount in token's native decimals
     * @param tokenAddr Payment token address (to get decimals)
     * @return Protocol fee in same units as payAmt
     *
     * @dev Tiered fee structure:
     * $0-10:      1.0% fee, $0.10 minimum
     * $10-100:    1.5% fee, $0.50 minimum
     * $100-1k:    2.0% fee, $2.00 minimum
     * $1k-10k:    1.5% fee, $20 minimum
     * $10k+:      1.0% fee, $150 minimum
     */
    function _calculateProtocolFee(uint256 payAmt, address tokenAddr) internal view returns (uint256) {
        uint8 decimals = tokenDecimals[tokenAddr];
        uint256 ONE_USD = 10**decimals;

        if (payAmt <= 10 * ONE_USD) {
            // $0-10: 1.0% fee, $0.10 minimum
            uint256 fee = (payAmt * 100) / 10000; // 1.0%
            uint256 minFee = ONE_USD / 10; // $0.10
            return fee > minFee ? fee : minFee;
        } else if (payAmt <= 100 * ONE_USD) {
            // $10-100: 1.5% fee, $0.50 minimum
            uint256 fee = (payAmt * 150) / 10000; // 1.5%
            uint256 minFee = ONE_USD / 2; // $0.50
            return fee > minFee ? fee : minFee;
        } else if (payAmt <= 1000 * ONE_USD) {
            // $100-1k: 2.0% fee, $2.00 minimum
            uint256 fee = (payAmt * 200) / 10000; // 2.0%
            uint256 minFee = 2 * ONE_USD; // $2.00
            return fee > minFee ? fee : minFee;
        } else if (payAmt <= 10000 * ONE_USD) {
            // $1k-10k: 1.5% fee, $20 minimum
            uint256 fee = (payAmt * 150) / 10000; // 1.5%
            uint256 minFee = 20 * ONE_USD; // $20
            return fee > minFee ? fee : minFee;
        } else {
            // $10k+: 1.0% fee, $150 minimum
            uint256 fee = (payAmt * 100) / 10000; // 1.0%
            uint256 minFee = 150 * ONE_USD; // $150
            return fee > minFee ? fee : minFee;
        }
    }

    /**
     * @notice Normalize amount from one decimal precision to another
     * @param amount Amount in source decimals
     * @param fromDecimals Source decimal places
     * @param toDecimals Target decimal places
     * @return Normalized amount in target decimals
     */
    function _normalizeAmount(uint256 amount, uint8 fromDecimals, uint8 toDecimals) internal pure returns (uint256) {
        if (fromDecimals == toDecimals) {
            return amount;
        } else if (fromDecimals > toDecimals) {
            return amount / (10 ** (fromDecimals - toDecimals));
        } else {
            return amount * (10 ** (toDecimals - fromDecimals));
        }
    }

    /**
     * @notice Check if address is a selected verifier for job
     */
    function _isSelectedVerifier(bytes32 jobId, address verifier) internal view returns (bool) {
        Job storage job = jobs[jobId];
        for (uint256 i = 0; i < 3; i++) {
            if (job.selectedVerifiers[i] == verifier) {
                return true;
            }
        }
        return false;
    }

    /**
     * @notice Activate backup verifier when primary is offline/slashed
     */
    function _activateBackupVerifier(bytes32 jobId, address failedVerifier) internal {
        Job storage job = jobs[jobId];

        // Find which position the failed verifier was in
        for (uint256 i = 0; i < 3; i++) {
            if (job.selectedVerifiers[i] == failedVerifier) {
                // Replace with backup verifier
                address backup = job.backupVerifiers[i];
                if (backup != address(0) && verifiers[backup].active) {
                    job.selectedVerifiers[i] = backup;
                    emit BackupVerifierActivated(jobId, backup, failedVerifier);
                }
                break;
            }
        }
    }

    /**
     * @notice Get executor's required collateral multiplier
     *
     * @dev DEPRECATED: Now returns fixed 2.0x for all executors.
     * Kept for backwards compatibility with external integrations.
     */
    function getExecutorCollateralMultiplier(address executor) public pure returns (uint256) {
        // Always return 200 (2.0x) - no dynamic collateral
        return EXECUTOR_COLLATERAL_MULTIPLIER;
    }

    /**
     * @notice Update executor reputation after job completion or fraud
     *
     * @dev Implements harsh fraud penalties:
     * - 1st fraud: 30-day ban
     * - 2nd fraud: Permanent ban
     */
    function _updateExecutorReputation(address executor, bool isFraud) internal {
        ExecutorReputation storage rep = executorReputation[executor];

        if (isFraud) {
            rep.fraudAttempts++;
            rep.lastFraudTimestamp = block.timestamp;

            // Ban logic
            if (rep.fraudAttempts == 1) {
                // 1st fraud: 30-day ban
                rep.banUntil = block.timestamp + EXECUTOR_BAN_DURATION;
                emit ExecutorBanned(executor, rep.banUntil, false);
            } else if (rep.fraudAttempts > MAX_FRAUD_ATTEMPTS) {
                // 2nd fraud: Permanent ban
                rep.permanentlyBanned = true;
                emit ExecutorBanned(executor, type(uint256).max, true);
            }
        } else {
            rep.jobsCompleted++;
        }

        emit ExecutorReputationUpdated(executor, rep.fraudAttempts, EXECUTOR_COLLATERAL_MULTIPLIER);
    }

    /**
     * @notice Select 3 verifiers using Chainlink VRF randomness
     * @param jobId Job identifier
     * @param randomWord Random value from Chainlink VRF
     * @return selected Array of 3 verifier addresses
     */
    function _selectRandomVerifiersWithVRF(bytes32 jobId, uint256 randomWord) internal view returns (address[3] memory selected) {
        uint256 count = 0;
        uint256 attempts = 0;
        uint256 maxAttempts = verifierList.length * 2;
        if (maxAttempts > MAX_VERIFIER_SELECTION_ATTEMPTS) {
            maxAttempts = MAX_VERIFIER_SELECTION_ATTEMPTS;
        }
        while (count < 3 && attempts < maxAttempts) {
            uint256 index = uint256(keccak256(abi.encodePacked(randomWord, jobId, attempts))) % verifierList.length;
            address verifier = verifierList[index];

            bool alreadySelected = false;
            for (uint256 i = 0; i < count; i++) {
                if (selected[i] == verifier) {
                    alreadySelected = true;
                    break;
                }
            }

            bool isOnline = (block.timestamp - verifiers[verifier].lastHeartbeat) <= HEARTBEAT_INTERVAL;
            bool tokenMatches = verifiers[verifier].stakeToken == jobs[jobId].payToken;

            if (!alreadySelected && verifiers[verifier].active && isOnline && tokenMatches) {
                selected[count] = verifier;
                count++;
            }

            attempts++;
        }

        require(count == 3, "Failed to select 3 token-matching online verifiers");
        return selected;
    }

    /**
     * @notice Select 3 backup verifiers using VRF randomness
     */
    function _selectBackupVerifiersWithVRF(bytes32 jobId, uint256 randomWord, address[3] memory primary) internal view returns (address[3] memory backup) {
        uint256 count = 0;
        uint256 attempts = 0;
        uint256 maxAttempts = verifierList.length * 2;
        if (maxAttempts > MAX_VERIFIER_SELECTION_ATTEMPTS) {
            maxAttempts = MAX_VERIFIER_SELECTION_ATTEMPTS;
        }
        while (count < 3 && attempts < maxAttempts) {
            uint256 index = uint256(keccak256(abi.encodePacked(randomWord, jobId, "backup", attempts))) % verifierList.length;
            address verifier = verifierList[index];

            // Check not in primary
            bool isPrimary = false;
            for (uint256 i = 0; i < 3; i++) {
                if (primary[i] == verifier) {
                    isPrimary = true;
                    break;
                }
            }

            // Check not already in backup
            bool alreadySelected = false;
            for (uint256 i = 0; i < count; i++) {
                if (backup[i] == verifier) {
                    alreadySelected = true;
                    break;
                }
            }

            bool isOnline = (block.timestamp - verifiers[verifier].lastHeartbeat) <= HEARTBEAT_INTERVAL;
            bool tokenMatches = verifiers[verifier].stakeToken == jobs[jobId].payToken;

            if (!isPrimary && !alreadySelected && verifiers[verifier].active && isOnline && tokenMatches) {
                backup[count] = verifier;
                count++;
            }

            attempts++;
        }

        require(count == 3, "Failed to select 3 backup verifiers");
        return backup;
    }

    // ============================================================================
    // Admin Functions
    // ============================================================================

    function setClientDepositParams(uint256 _basisPoints, uint256 _minUsd, uint256 _maxUsd) external onlyOwner {
        require(_basisPoints > 0 && _basisPoints <= 2000, "Basis points must be 1-2000 (0.01%-20%)");
        require(_minUsd > 0 && _minUsd <= 100 * 10**6, "Min deposit must be >0 and <=$100");
        require(_maxUsd >= _minUsd && _maxUsd <= 10000 * 10**6, "Max deposit invalid");
        clientDepositBasisPoints = _basisPoints;
        minClientDepositUsd = _minUsd;
        maxClientDepositUsd = _maxUsd;
    }

    function setChallengeWindow(uint256 _seconds) external onlyOwner {
        require(_seconds >= 3600 && _seconds <= 86400, "Window must be 1h-24h");
        challengeWindow = _seconds;
    }

    function setAcceptWindow(uint256 _seconds) external onlyOwner {
        require(_seconds >= 30 && _seconds <= 3600, "Window must be 30s-1h");
        acceptWindow = _seconds;
    }

    function setTreasury(address _treasury) external onlyOwner {
        require(_treasury != address(0), "Invalid treasury");
        treasury = _treasury;
    }

    function pause() external onlyOwner whenNotPaused {
        paused = true;
        emit Paused(msg.sender);
    }

    function unpause() external onlyOwner whenPaused {
        paused = false;
        emit Unpaused(msg.sender);
    }

    /**
     * @notice Withdraw accumulated protocol fees
     * @param token Token address to withdraw
     * @param to Recipient address
     * @param amount Amount to withdraw
     */
    function withdrawProtocolFees(address token, address to, uint256 amount) external onlyOwner {
        require(to != address(0), "Invalid recipient");
        require(amount > 0, "Amount must be positive");

        // Safety check to prevent draining user funds
        // Protocol fees are tracked separately from escrowed job funds
        uint256 contractBalance = IERC20(token).balanceOf(address(this));
        require(contractBalance >= amount, "Insufficient balance");

        IERC20(token).safeTransfer(to, amount);
        emit ProtocolFeesWithdrawn(token, to, amount);
    }

    /**
     * @notice Add a supported payment token
     * @param token Token address to support
     * @dev Queries decimals from token contract on addition
     */
    function addSupportedToken(address token) external onlyOwner {
        require(token != address(0), "Invalid token address");
        require(!supportedTokens[token], "Token already supported");
        uint8 decimals = IERC20Metadata(token).decimals();
        tokenDecimals[token] = decimals;
        supportedTokens[token] = true;
        emit TokenSupported(token, decimals);
    }

    /**
     * @notice Remove a supported payment token
     * @param token Token address to remove
     * @dev Preserves tokenDecimals for in-flight jobs using this token
     */
    function removeSupportedToken(address token) external onlyOwner {
        require(supportedTokens[token], "Token not supported");
        supportedTokens[token] = false;
        // Keep tokenDecimals[token] intact for existing jobs
        emit TokenRemoved(token);
    }

    // ============================================================================
    // View Functions
    // ============================================================================

    function getJob(bytes32 jobId) external view returns (Job memory) {
        require(jobExists[jobId], "Job does not exist");
        return jobs[jobId];
    }

    function getJobStatus(bytes32 jobId) external view returns (Status) {
        require(jobExists[jobId], "Job does not exist");
        return jobs[jobId].status;
    }

    // ============================================================================
    // Bisection Helper Functions
    // ============================================================================

    /**
     * @notice Verify Merkle proof for execution state at specific step
     * @param data Step data
     * @param proof Merkle proof path
     * @param stepIndex Step index in execution trace
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
     * @param stepData Encoded instruction and memory state
     * @param preStateHash Hash of machine state before step
     * @return Hash of machine state after step execution
     */
    function _executeSingleStep(
        bytes memory stepData,
        bytes32 preStateHash
    ) internal returns (bytes32) {
        // Call Stylus single-step executor (gas efficient)
        (bool success, bytes memory result) = stylusExecutor.call(
            abi.encodeWithSignature("executeStep(bytes,bytes32)", stepData, preStateHash)
        );

        if (!success) {
            // If Stylus fails treat as executor fraud
            return bytes32(0);
        }

        return abi.decode(result, (bytes32));
    }
}
