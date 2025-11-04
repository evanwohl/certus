// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "./CertusBase.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/**
 * @title CertusVerifier
 * @notice Verifier management and selection logic
 */
contract CertusVerifier is CertusBase, ReentrancyGuard {
    using SafeERC20 for IERC20;

    // Verifier state
    mapping(address => VerifierStake) public verifiers;
    address[] public verifierList;
    mapping(uint8 => uint256) public verifierCountByRegion;

    // VRF integration
    address public immutable vrfCoordinator;
    bytes32 public immutable vrfKeyHash;
    uint64 public immutable vrfSubId;

    // Access control
    address public escrowContract;
    mapping(uint256 => bytes32) public vrfRequestToJobId;
    mapping(bytes32 => uint256) public jobIdToVrfRequest;
    mapping(uint256 => bool) public vrfRequestFulfilled;

    // Verifier tracking
    mapping(bytes32 => mapping(address => bool)) public verifierResponded;

    // Events
    event VerifierHeartbeat(address indexed verifier, uint256 timestamp);
    event BackupVerifierActivated(bytes32 indexed jobId, address indexed backup, address indexed replaced);
    event FallbackVerifierSelection(bytes32 indexed jobId, uint256 blocksSinceReceipt);

    modifier onlyVRF() {
        require(msg.sender == vrfCoordinator, "Only VRF coordinator");
        _;
    }

    modifier onlyEscrow() {
        require(msg.sender == escrowContract, "Only escrow");
        _;
    }

    constructor(
        address _vrfCoordinator,
        bytes32 _vrfKeyHash,
        uint64 _vrfSubId
    ) {
        vrfCoordinator = _vrfCoordinator;
        vrfKeyHash = _vrfKeyHash;
        vrfSubId = _vrfSubId;
    }

    /**
     * Set escrow contract (one-time)
     */
    function setEscrowContract(address _escrowContract) external {
        require(escrowContract == address(0), "Already set");
        require(_escrowContract != address(0), "Invalid address");
        escrowContract = _escrowContract;
    }

    /**
     * Register as verifier with stake
     */
    function registerVerifier(
        address stakeToken,
        uint256 amount,
        uint256 storageCapacityGB,
        uint8 region
    ) external nonReentrant {
        require(amount >= MIN_VERIFIER_STAKE, "Insufficient stake");
        require(storageCapacityGB >= 10, "Minimum 10GB storage required");
        require(!verifiers[msg.sender].active, "Already registered");

        // Check geographic concentration (account for new joiner)
        uint256 newRegionCount = verifierCountByRegion[region] + 1;
        uint256 newTotalCount = verifierList.length + 1;
        require(newRegionCount <= (newTotalCount * MAX_REGION_CONCENTRATION) / 100,
                "Region concentration exceeded");

        IERC20(stakeToken).safeTransferFrom(msg.sender, address(this), amount);

        verifiers[msg.sender] = VerifierStake({
            amount: amount,
            stakeToken: stakeToken,
            active: true,
            lastHeartbeat: block.timestamp,
            storageCapacityGB: storageCapacityGB,
            region: region,
            jobsVerified: 0,
            fraudsDetected: 0
        });

        verifierList.push(msg.sender);
        verifierCountByRegion[region]++;

        emit VerifierRegistered(msg.sender, amount, storageCapacityGB);
    }

    /**
     * Verifier heartbeat to maintain active status
     */
    function heartbeat() external {
        require(verifiers[msg.sender].active, "Not active verifier");
        verifiers[msg.sender].lastHeartbeat = block.timestamp;
        emit VerifierHeartbeat(msg.sender, block.timestamp);
    }

    /**
     * Select verifiers using VRF randomness
     */
    function selectVerifiersWithVRF(
        bytes32 jobId,
        address payToken,
        uint256 randomWord1,
        uint256 randomWord2
    ) external returns (address[3] memory selected, address[3] memory backup) {
        selected = _selectPrimaryVerifiers(jobId, payToken, randomWord1);
        backup = _selectBackupVerifiers(jobId, payToken, randomWord2, selected);
        return (selected, backup);
    }

    /**
     * Check if address is selected verifier
     */
    function isSelectedVerifier(bytes32 /* jobId */, address verifier, address[3] memory selected)
        external pure returns (bool) {
        for (uint256 i = 0; i < 3; i++) {
            if (selected[i] == verifier) return true;
        }
        return false;
    }

    /**
     * Slash verifier for non-response
     */
    function slashVerifier(
        address verifier,
        address reporter,
        uint256 penalty
    ) external onlyEscrow returns (bool) {
        require(verifiers[verifier].active, "Not active verifier");

        uint256 stake = verifiers[verifier].amount;
        require(stake >= penalty, "Insufficient stake to slash");

        verifiers[verifier].amount -= penalty;
        verifiers[verifier].active = false;

        // Pay reporter
        IERC20(verifiers[verifier].stakeToken).safeTransfer(reporter, penalty);

        return true;
    }

    /**
     * Unregister verifier and return stake
     */
    function unregisterVerifier() external nonReentrant {
        require(verifiers[msg.sender].active, "Not active verifier");

        VerifierStake storage stake = verifiers[msg.sender];
        uint256 refundAmount = stake.amount;
        address stakeToken = stake.stakeToken;
        uint8 region = stake.region;

        stake.active = false;
        stake.amount = 0;
        verifierCountByRegion[region]--;

        // Remove from list
        for (uint256 i = 0; i < verifierList.length; i++) {
            if (verifierList[i] == msg.sender) {
                verifierList[i] = verifierList[verifierList.length - 1];
                verifierList.pop();
                break;
            }
        }

        IERC20(stakeToken).safeTransfer(msg.sender, refundAmount);
        emit VerifierUnregistered(msg.sender, refundAmount);
    }

    /**
     * Select 3 primary verifiers
     */
    function _selectPrimaryVerifiers(
        bytes32 jobId,
        address payToken,
        uint256 randomWord
    ) internal view returns (address[3] memory selected) {
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
            bool tokenMatches = verifiers[verifier].stakeToken == payToken;

            if (!alreadySelected && verifiers[verifier].active && isOnline && tokenMatches) {
                selected[count] = verifier;
                count++;
            }

            attempts++;
        }

        require(count == 3, "Failed to select 3 verifiers");
        return selected;
    }

    /**
     * Select 3 backup verifiers
     */
    function _selectBackupVerifiers(
        bytes32 jobId,
        address payToken,
        uint256 randomWord,
        address[3] memory primary
    ) internal view returns (address[3] memory backup) {
        uint256 count = 0;
        uint256 attempts = 0;
        uint256 maxAttempts = verifierList.length * 2;
        if (maxAttempts > MAX_VERIFIER_SELECTION_ATTEMPTS) {
            maxAttempts = MAX_VERIFIER_SELECTION_ATTEMPTS;
        }

        while (count < 3 && attempts < maxAttempts) {
            uint256 index = uint256(keccak256(abi.encodePacked(randomWord, jobId, "backup", attempts))) % verifierList.length;
            address verifier = verifierList[index];

            bool isPrimary = false;
            for (uint256 i = 0; i < 3; i++) {
                if (primary[i] == verifier || backup[i] == verifier) {
                    isPrimary = true;
                    break;
                }
            }

            bool isOnline = (block.timestamp - verifiers[verifier].lastHeartbeat) <= HEARTBEAT_INTERVAL;
            bool tokenMatches = verifiers[verifier].stakeToken == payToken;

            if (!isPrimary && verifiers[verifier].active && isOnline && tokenMatches) {
                backup[count] = verifier;
                count++;
            }

            attempts++;
        }

        // Backups are optional
        return backup;
    }
}