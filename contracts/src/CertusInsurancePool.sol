// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/**
 * Insurance pool for executor slashing protection
 * Funded by 10% of protocol fees, pays out 50% of slashing losses
 */
contract CertusInsurancePool is ReentrancyGuard {
    using SafeERC20 for IERC20;

    // Constants
    uint256 public constant STAKE_REQUIREMENT = 50_000 * 10**18; // 50k CERTUS
    uint256 public constant COVERAGE_RATIO = 50; // 50% coverage
    uint256 public constant PROTOCOL_FEE_SHARE = 10; // 10% of fees go to pool
    uint256 public constant MIN_POOL_BALANCE = 100_000 * 10**6; // $100k USDC minimum
    uint256 public constant MAX_PAYOUT_PER_INCIDENT = 10_000 * 10**6; // $10k cap per incident

    // State
    IERC20 public immutable certusToken;
    IERC20 public immutable usdc;
    address public immutable escrow;

    mapping(address => uint256) public stakedAmount;
    mapping(address => uint256) public lastClaimTime;
    mapping(address => uint256) public totalClaimed;

    uint256 public poolBalance;
    uint256 public totalStaked;
    uint256 public totalPayouts;

    // Events
    event ExecutorStaked(address indexed executor, uint256 amount);
    event ExecutorUnstaked(address indexed executor, uint256 amount);
    event InsurancePayout(address indexed executor, uint256 slashAmount, uint256 payoutAmount);
    event PoolFunded(uint256 amount);

    modifier onlyEscrow() {
        require(msg.sender == escrow, "Only escrow");
        _;
    }

    constructor(address _certusToken, address _usdc, address _escrow) {
        certusToken = IERC20(_certusToken);
        usdc = IERC20(_usdc);
        escrow = _escrow;
    }

    /**
     * Stake CERTUS to join insurance pool
     */
    function stake(uint256 amount) external nonReentrant {
        require(amount >= STAKE_REQUIREMENT, "Insufficient stake");

        certusToken.safeTransferFrom(msg.sender, address(this), amount);
        stakedAmount[msg.sender] += amount;
        totalStaked += amount;

        emit ExecutorStaked(msg.sender, amount);
    }

    /**
     * Unstake and leave insurance pool
     */
    function unstake() external nonReentrant {
        uint256 staked = stakedAmount[msg.sender];
        require(staked > 0, "Not staked");
        require(block.timestamp > lastClaimTime[msg.sender] + 30 days, "Recent claim");

        stakedAmount[msg.sender] = 0;
        totalStaked -= staked;

        certusToken.safeTransfer(msg.sender, staked);
        emit ExecutorUnstaked(msg.sender, staked);
    }

    /**
     * Process insurance payout for slashed executor
     */
    function processPayout(address executor, uint256 slashAmount) external onlyEscrow returns (uint256) {
        // Check eligibility
        if (stakedAmount[executor] < STAKE_REQUIREMENT) {
            return 0;
        }

        // Calculate payout (50% of slash, capped)
        uint256 payoutAmount = (slashAmount * COVERAGE_RATIO) / 100;
        payoutAmount = payoutAmount > MAX_PAYOUT_PER_INCIDENT ? MAX_PAYOUT_PER_INCIDENT : payoutAmount;

        // Check pool has funds
        if (poolBalance < payoutAmount) {
            payoutAmount = poolBalance; // Pay what we can
        }

        if (payoutAmount > 0) {
            poolBalance -= payoutAmount;
            totalPayouts += payoutAmount;
            totalClaimed[executor] += payoutAmount;
            lastClaimTime[executor] = block.timestamp;

            // Transfer payout
            usdc.safeTransfer(executor, payoutAmount);
            emit InsurancePayout(executor, slashAmount, payoutAmount);
        }

        return payoutAmount;
    }

    /**
     * Fund pool with protocol fees
     */
    function fundPool(uint256 amount) external onlyEscrow {
        usdc.safeTransferFrom(msg.sender, address(this), amount);
        poolBalance += amount;
        emit PoolFunded(amount);
    }

    /**
     * Check if executor is eligible
     */
    function isEligible(address executor) external view returns (bool) {
        return stakedAmount[executor] >= STAKE_REQUIREMENT;
    }

    /**
     * Get pool health metrics
     */
    function getPoolHealth() external view returns (uint256 balance, uint256 stakers, bool isHealthy) {
        balance = poolBalance;
        stakers = totalStaked / STAKE_REQUIREMENT;
        isHealthy = poolBalance >= MIN_POOL_BALANCE;
    }
}