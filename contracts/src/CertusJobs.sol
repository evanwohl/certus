// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./CertusBase.sol";
import "./CertusVerifier.sol";
import "./CertusBisection.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@chainlink/contracts/src/v0.8/interfaces/VRFCoordinatorV2Interface.sol";

interface IVRFCoordinator {
    function requestRandomWords(
        bytes32 keyHash,
        uint64 subId,
        uint16 minimumRequestConfirmations,
        uint32 callbackGasLimit,
        uint32 numWords
    ) external returns (uint256 requestId);
}

interface IStylusWasmExecutor {
    function execute(
        bytes calldata wasm,
        bytes calldata input,
        uint64 fuelLimit,
        uint64 memLimit
    ) external returns (bytes memory output);
}

/**
 * @title CertusJobs
 * @notice Core job lifecycle and escrow management
 */
contract CertusJobs is CertusBase, ReentrancyGuard, Ownable {
    using SafeERC20 for IERC20;

    // Job storage
    mapping(bytes32 => Job) public jobs;
    mapping(bytes32 => bool) public jobExists;

    // Receipt tracking
    mapping(bytes32 => uint256) public receiptTimestamp;
    mapping(bytes32 => uint256) public receiptBlockNumber;

    // Client griefing tracking
    mapping(address => uint256) public clientGriefCount;

    // Executor reputation
    mapping(address => ExecutorReputation) public executorReputation;

    // Token support
    mapping(address => bool) public supportedTokens;
    mapping(address => uint8) public tokenDecimals;

    // Accounting isolation
    mapping(address => uint256) public totalEscrowedFunds;
    mapping(address => uint256) public protocolFeesAccumulated;

    // Protocol stats
    uint256 public totalJobsCreated;
    uint256 public totalJobsFinalized;
    uint256 public totalJobsSlashed;

    // External contracts
    CertusVerifier public immutable verifierContract;
    CertusBisection public immutable bisectionContract;
    address public immutable stylusExecutor;
    address public escrowContract; // Set after deployment

    // VRF integration
    address public immutable vrfCoordinator;
    bytes32 public immutable vrfKeyHash;
    uint64 public immutable vrfSubId;
    mapping(uint256 => bytes32) public vrfRequestToJobId;
    mapping(bytes32 => uint256) public jobIdToVrfRequest;
    mapping(uint256 => bool) public vrfRequestFulfilled;

    // Emergency pause
    bool public paused;

    // Fee parameters
    uint256 public minClientDepositUsd = 5 * 10**6; // $5
    uint256 public maxClientDepositUsd = 1000 * 10**6; // $1000
    uint256 public clientDepositBasisPoints = 500; // 5%

    modifier whenNotPaused() {
        require(!paused, "Contract paused");
        _;
    }

    constructor(
        address _verifierContract,
        address _bisectionContract,
        address _stylusExecutor,
        address _vrfCoordinator,
        bytes32 _vrfKeyHash,
        uint64 _vrfSubId
    ) {
        verifierContract = CertusVerifier(_verifierContract);
        bisectionContract = CertusBisection(_bisectionContract);
        stylusExecutor = _stylusExecutor;
        vrfCoordinator = _vrfCoordinator;
        vrfKeyHash = _vrfKeyHash;
        vrfSubId = _vrfSubId;
    }

    /**
     * Create new job with escrow
     */
    function createJob(
        bytes32 jobId,
        bytes32 wasmHash,
        bytes32 inputHash,
        address payToken,
        uint256 payAmt,
        uint64 acceptWindow,
        uint64 challengeWindow,
        uint64 fuelLimit,
        uint64 memLimit,
        uint32 maxOutputSize
    ) external whenNotPaused nonReentrant {
        require(!jobExists[jobId], "Job already exists");
        require(supportedTokens[payToken], "Token not supported");
        require(payAmt > 0, "Payment must be positive");
        require(challengeWindow >= 3600, "Challenge window too short");

        // Calculate client deposit
        uint8 decimals = tokenDecimals[payToken];
        uint256 proportionalDeposit = (payAmt * clientDepositBasisPoints) / 10000;
        uint256 minDeposit = normalizeAmount(minClientDepositUsd, 6, decimals);
        uint256 maxDeposit = normalizeAmount(maxClientDepositUsd, 6, decimals);

        uint256 clientDeposit = proportionalDeposit;
        if (clientDeposit < minDeposit) clientDeposit = minDeposit;
        if (clientDeposit > maxDeposit) clientDeposit = maxDeposit;

        uint256 totalClientPayment = payAmt + clientDeposit;

        // Transfer funds
        IERC20(payToken).safeTransferFrom(msg.sender, address(this), totalClientPayment);
        totalEscrowedFunds[payToken] += totalClientPayment;

        // Create job
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
            acceptDeadline: uint64(block.timestamp) + acceptWindow,
            finalizeDeadline: uint64(block.timestamp) + acceptWindow + challengeWindow,
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
     * Executor accepts job
     */
    function acceptJob(bytes32 jobId) external whenNotPaused nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Created, "Job not available");
        require(block.timestamp <= job.acceptDeadline, "Accept deadline passed");

        // Check executor reputation
        ExecutorReputation storage rep = executorReputation[msg.sender];
        require(!rep.permanentlyBanned, "Executor permanently banned");
        require(block.timestamp >= rep.banUntil, "Executor temporarily banned");

        // Fixed 2x collateral
        require(job.payAmt <= type(uint256).max / 2, "Payment amount too large");
        uint256 executorDeposit = job.payAmt * 2;

        // Transfer collateral
        IERC20(job.payToken).safeTransferFrom(msg.sender, address(this), executorDeposit);
        totalEscrowedFunds[job.payToken] += executorDeposit;

        job.executor = msg.sender;
        job.executorDeposit = executorDeposit;
        job.status = Status.Accepted;

        emit JobAccepted(jobId, msg.sender, executorDeposit);
    }

    /**
     * Submit execution receipt
     */
    function submitReceipt(
        bytes32 jobId,
        bytes32 outputHash,
        bytes calldata execSig,
        uint64 challengeWindow
    ) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Accepted, "Job not accepted");
        require(msg.sender == job.executor, "Only executor can submit");

        job.outputHash = outputHash;
        job.status = Status.Receipt;
        job.finalizeDeadline = uint64(block.timestamp) + challengeWindow;
        receiptTimestamp[jobId] = block.timestamp;
        receiptBlockNumber[jobId] = block.number;

        // Request VRF for verifier selection
        uint256 requestId = IVRFCoordinator(vrfCoordinator).requestRandomWords(
            vrfKeyHash,
            vrfSubId,
            3,
            500000,
            2
        );

        vrfRequestToJobId[requestId] = jobId;
        jobIdToVrfRequest[jobId] = requestId;

        emit ReceiptSubmitted(jobId, outputHash, execSig);
    }

    /**
     * VRF callback
     */
    function fulfillRandomWords(uint256 requestId, uint256[] memory randomWords) external {
        require(msg.sender == vrfCoordinator, "Only VRF coordinator");
        require(!vrfRequestFulfilled[requestId], "Already fulfilled");

        bytes32 jobId = vrfRequestToJobId[requestId];
        Job storage job = jobs[jobId];

        (address[3] memory selected, address[3] memory backup) =
            verifierContract.selectVerifiersWithVRF(jobId, job.payToken, randomWords[0], randomWords[1]);

        job.selectedVerifiers = selected;
        job.backupVerifiers = backup;
        vrfRequestFulfilled[requestId] = true;

        emit VerifiersSelected(jobId, selected, backup);
    }

    /**
     * Finalize job
     */
    function finalize(bytes32 jobId) external nonReentrant {
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Not in receipt state");
        require(msg.sender == job.client, "Only client can finalize");
        require(block.timestamp <= job.finalizeDeadline, "Deadline passed");

        // Calculate fee
        uint256 protocolFee = calculateProtocolFee(job.payAmt, job.payToken);
        uint256 executorPayment = job.payAmt - protocolFee + job.executorDeposit;

        job.status = Status.Finalized;
        totalJobsFinalized++;

        // Update accounting
        totalEscrowedFunds[job.payToken] -= (job.payAmt + job.executorDeposit + job.clientDeposit);
        protocolFeesAccumulated[job.payToken] += protocolFee;

        // Transfers
        IERC20(job.payToken).safeTransfer(job.executor, executorPayment);
        IERC20(job.payToken).safeTransfer(job.client, job.clientDeposit);

        emit JobFinalized(jobId, job.executor, executorPayment);
    }

    /**
     * Calculate protocol fee
     */
    function calculateProtocolFee(uint256 payAmt, address tokenAddr) public view returns (uint256) {
        uint8 decimals = tokenDecimals[tokenAddr];
        uint256 ONE_USD = 10**decimals;

        if (payAmt <= TIER1_MAX * ONE_USD) {
            uint256 fee = (payAmt * 100) / 10000; // 1%
            uint256 minFee = ONE_USD / 10; // $0.10
            return fee > minFee ? fee : minFee;
        } else if (payAmt <= TIER2_MAX * ONE_USD) {
            uint256 fee = (payAmt * 150) / 10000; // 1.5%
            uint256 minFee = ONE_USD / 2; // $0.50
            return fee > minFee ? fee : minFee;
        } else if (payAmt <= TIER3_MAX * ONE_USD) {
            uint256 fee = (payAmt * 200) / 10000; // 2%
            uint256 minFee = 2 * ONE_USD; // $2
            return fee > minFee ? fee : minFee;
        } else if (payAmt <= TIER4_MAX * ONE_USD) {
            uint256 fee = (payAmt * 150) / 10000; // 1.5%
            uint256 minFee = 20 * ONE_USD; // $20
            return fee > minFee ? fee : minFee;
        } else {
            uint256 fee = (payAmt * 100) / 10000; // 1%
            uint256 minFee = 150 * ONE_USD; // $150
            return fee > minFee ? fee : minFee;
        }
    }

    /**
     * Normalize token amounts between different decimal scales
     * @param amount Amount to normalize
     * @param fromDecimals Source token decimals
     * @param toDecimals Target token decimals
     * @return normalized The normalized amount
     */
    function normalizeAmount(uint256 amount, uint8 fromDecimals, uint8 toDecimals)
        public pure returns (uint256 normalized) {
        if (fromDecimals == toDecimals) {
            return amount;
        } else if (fromDecimals > toDecimals) {
            return amount / (10 ** (fromDecimals - toDecimals));
        } else {
            return amount * (10 ** (toDecimals - fromDecimals));
        }
    }

    /**
     * Set escrow contract address (one-time)
     */
    function setEscrowContract(address _escrowContract) external onlyOwner {
        require(escrowContract == address(0), "Already set");
        require(_escrowContract != address(0), "Invalid address");
        escrowContract = _escrowContract;
    }

    /**
     * Mark job as slashed (only callable by escrow)
     */
    function markSlashed(bytes32 jobId) external {
        require(msg.sender == escrowContract, "Only escrow");
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Invalid status");
        job.status = Status.Slashed;
        totalJobsSlashed++;

        // Update accounting
        totalEscrowedFunds[job.payToken] -= (job.payAmt + job.executorDeposit + job.clientDeposit);
    }

    /**
     * Mark job as finalized (only callable by escrow)
     */
    function markFinalized(bytes32 jobId) external {
        require(msg.sender == escrowContract, "Only escrow");
        Job storage job = jobs[jobId];
        require(job.status == Status.Receipt, "Invalid status");
        job.status = Status.Finalized;
        totalJobsFinalized++;

        // Update accounting
        uint256 protocolFee = calculateProtocolFee(job.payAmt, job.payToken);
        totalEscrowedFunds[job.payToken] -= (job.payAmt + job.executorDeposit + job.clientDeposit);
        protocolFeesAccumulated[job.payToken] += protocolFee;
    }

    /**
     * Get job details
     */
    function getJob(bytes32 jobId) external view returns (Job memory) {
        require(jobExists[jobId], "Job does not exist");
        return jobs[jobId];
    }

    /**
     * Emergency pause
     */
    function pause() external onlyOwner whenNotPaused {
        paused = true;
        emit Paused(msg.sender);
    }

    function unpause() external onlyOwner {
        require(paused, "Not paused");
        paused = false;
        emit Unpaused(msg.sender);
    }

    // Events
    event Paused(address indexed by);
    event Unpaused(address indexed by);
}