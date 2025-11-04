// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

interface ISwapRouter {
    struct ExactInputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24 fee;
        address recipient;
        uint256 deadline;
        uint256 amountIn;
        uint256 amountOutMinimum;
        uint160 sqrtPriceLimitX96;
    }

    function exactInputSingle(ExactInputSingleParams calldata params) external payable returns (uint256 amountOut);
}

/**
 * @title CertusToken
 * @notice CERTUS governance token with 7 utility mechanisms and veCERTUS staking
 * @dev Fixed supply: 100,000,000 tokens. No inflation post-launch.
 *
 * Distribution:
 * - 30% (30M) Protocol Treasury
 * - 25% (25M) Liquidity Mining (Verifier Rewards)
 * - 20% (20M) Team & Advisors (12-month cliff, 48-month vest)
 * - 15% (15M) DEX Liquidity
 * - 10% (10M) Community Airdrop
 *
 * Utility Mechanisms:
 * 1. Verifier Selection Boost (stake 10k/50k/200k for 1.2x/1.5x/2.0x weight, prevents whale dominance)
 * 2. Fee Discount (pay 50%/100% in CERTUS for 25%/40% discount)
 * 3. Governance (1 CERTUS = 1 vote)
 * 4. Revenue Share (veCERTUS holders get 50% protocol fees in USDC)
 * 5. Executor Insurance Pool (stake 50k for 50% slashing coverage, no capital efficiency advantage)
 * 6. Priority Execution (burn 100 CERTUS to skip queue)
 * 7. Buyback & Burn (30% protocol fees for deflationary pressure)
 */
contract CertusToken is ERC20, Ownable, ReentrancyGuard {
    using SafeERC20 for IERC20;

    // ============================================================================
    // Constants
    // ============================================================================

    uint256 public constant TOTAL_SUPPLY = 100_000_000 * 10**18; // 100M tokens

    // Distribution amounts
    uint256 public constant TREASURY_ALLOCATION = 30_000_000 * 10**18;
    uint256 public constant LIQUIDITY_MINING_ALLOCATION = 25_000_000 * 10**18;
    uint256 public constant TEAM_ALLOCATION = 20_000_000 * 10**18;
    uint256 public constant DEX_LIQUIDITY_ALLOCATION = 15_000_000 * 10**18;
    uint256 public constant AIRDROP_ALLOCATION = 10_000_000 * 10**18;

    // Vesting parameters
    uint256 public constant TEAM_CLIFF_DURATION = 365 days; // 12 months
    uint256 public constant TEAM_VEST_DURATION = 1461 days; // 48 months
    uint256 public constant MINING_VEST_DURATION = 1461 days; // 48 months

    // veCERTUS lock durations
    uint256 public constant MIN_LOCK_DURATION = 1 weeks;
    uint256 public constant MAX_LOCK_DURATION = 4 * 365 days; // 4 years

    // Utility thresholds (tiered boost prevents whale dominance)
    uint256 public constant VERIFIER_BOOST_TIER1 = 10_000 * 10**18; // 1.2x selection weight
    uint256 public constant VERIFIER_BOOST_TIER2 = 50_000 * 10**18; // 1.5x selection weight
    uint256 public constant VERIFIER_BOOST_TIER3 = 200_000 * 10**18; // 2.0x selection weight (max)
    uint256 public constant EXECUTOR_INSURANCE_STAKE = 50_000 * 10**18; // Join insurance pool
    uint256 public constant PRIORITY_BURN_AMOUNT = 100 * 10**18; // 100 CERTUS per priority job

    // Fee discount rates (basis points)
    uint256 public constant FEE_DISCOUNT_50PCT_CERTUS = 2500; // 25% total discount
    uint256 public constant FEE_DISCOUNT_100PCT_CERTUS = 4000; // 40% total discount

    // ============================================================================
    // Structs
    // ============================================================================

    struct VestingSchedule {
        uint256 total;
        uint256 released;
        uint256 startTime;
        uint256 cliffDuration;
        uint256 vestDuration;
    }

    struct VeCertusLock {
        uint256 amount;          // CERTUS locked
        uint256 unlockTime;      // Unix timestamp
        uint256 veCertusBalance; // Calculated veCERTUS balance
        uint256 lockDuration;    // Original lock duration
    }

    struct RevenueDistribution {
        uint256 totalDistributed;  // Total USDC distributed to date
        uint256 lastDistribution;  // Timestamp of last distribution
        uint256 accRevenuePerVeCertus; // Accumulated revenue per veCERTUS (scaled by 1e18)
    }

    // ============================================================================
    // State Variables
    // ============================================================================

    address public treasury;
    address public liquidityMiningPool;
    address public teamWallet;
    address public dexLiquidityPool;
    address public airdropContract;
    address public uniswapRouter; // Uniswap V3 router for buybacks

    uint256 public deployTime;
    uint256 public liquidityMiningStartTime;

    mapping(address => VestingSchedule) public vestingSchedules;
    mapping(address => VeCertusLock) public veCertusLocks;
    mapping(address => uint256) public revenueDebt; // For revenue distribution tracking

    RevenueDistribution public revenueDistribution;

    uint256 public totalVeCertus; // Total veCERTUS supply
    uint256 public totalBurned; // Deflationary tracking

    // Revenue tokens (USDC/USDT/DAI)
    address public revenueToken; // USDC for revenue share

    // Governance
    mapping(address => uint256) public votingPower; // 1 CERTUS = 1 vote + veCERTUS boost
    mapping(uint256 => mapping(address => bool)) public hasVoted; // proposalId => voter => voted

    // SECURITY: Track payment volume (not job count) for fee multipliers
    uint256 public monthlyPaymentVolume; // In USDC (6 decimals)
    uint256 public lastVolumeResetTime;

    // Dynamic subsidy and backloaded emissions
    uint256 public constant TARGET_VERIFIER_INCOME = 200 * 10**6; // $200 USDC per month minimum
    uint256 public activeVerifierCount; // Updated by escrow contract
    uint256 public totalMonthlyFees; // Total fees collected this month (USDC, 6 decimals)
    uint256 public lastSubsidyCalculation; // Timestamp of last subsidy calc

    // ============================================================================
    // Events
    // ============================================================================

    event VestedTokensClaimed(address indexed beneficiary, uint256 amount);
    event VeCertusLocked(address indexed user, uint256 amount, uint256 unlockTime, uint256 veCertusAmount);
    event VeCertusUnlocked(address indexed user, uint256 amount);
    event RevenueDistributed(uint256 amount, uint256 accRevenuePerVeCertus);
    event RevenueClaimed(address indexed user, uint256 amount);
    event TokensBurned(address indexed burner, uint256 amount, string reason);
    event BuybackExecuted(uint256 usdcSpent, uint256 certusBought, uint256 certusBurned);
    event VerifierBoostApplied(address indexed verifier, uint256 boostMultiplier);
    event ExecutorEfficiencyApplied(address indexed executor, uint256 collateralMultiplier);
    event FeeDiscountApplied(address indexed client, uint256 discountBps);
    event InsurancePoolJoined(address indexed executor, uint256 stakeAmount);
    event InsurancePoolPayout(address indexed executor, uint256 coverageAmount);
    event DynamicSubsidyCalculated(uint256 subsidyPerVerifier, uint256 totalSubsidy);

    // ============================================================================
    // Constructor
    // ============================================================================

    constructor(
        address _treasury,
        address _liquidityMiningPool,
        address _teamWallet,
        address _dexLiquidityPool,
        address _airdropContract,
        address _revenueToken,
        address _uniswapRouter
    ) ERC20("Certus", "CERTUS") Ownable(msg.sender) {
        require(_treasury != address(0), "Invalid treasury");
        require(_liquidityMiningPool != address(0), "Invalid mining pool");
        require(_teamWallet != address(0), "Invalid team wallet");
        require(_dexLiquidityPool != address(0), "Invalid DEX pool");
        require(_airdropContract != address(0), "Invalid airdrop");
        require(_revenueToken != address(0), "Invalid revenue token");
        require(_uniswapRouter != address(0), "Invalid Uniswap router");

        treasury = _treasury;
        liquidityMiningPool = _liquidityMiningPool;
        teamWallet = _teamWallet;
        dexLiquidityPool = _dexLiquidityPool;
        airdropContract = _airdropContract;
        revenueToken = _revenueToken;
        uniswapRouter = _uniswapRouter;

        deployTime = block.timestamp;
        liquidityMiningStartTime = block.timestamp;
        lastVolumeResetTime = block.timestamp;

        // Mint total supply
        _mint(address(this), TOTAL_SUPPLY);

        // Distribute immediately (without vesting)
        _transfer(address(this), _treasury, TREASURY_ALLOCATION);
        _transfer(address(this), _dexLiquidityPool, DEX_LIQUIDITY_ALLOCATION);
        _transfer(address(this), _airdropContract, AIRDROP_ALLOCATION);

        // Set up vesting for team
        vestingSchedules[_teamWallet] = VestingSchedule({
            total: TEAM_ALLOCATION,
            released: 0,
            startTime: block.timestamp,
            cliffDuration: TEAM_CLIFF_DURATION,
            vestDuration: TEAM_VEST_DURATION
        });

        // Liquidity mining held in contract for progressive release
    }

    // ============================================================================
    // Vesting Functions
    // ============================================================================

    /**
     * @notice Calculate vested amount for a beneficiary
     * @param beneficiary Address to check
     * @return Amount available to claim
     */
    function vestedAmount(address beneficiary) public view returns (uint256) {
        VestingSchedule memory schedule = vestingSchedules[beneficiary];
        if (schedule.total == 0) return 0;

        if (block.timestamp < schedule.startTime + schedule.cliffDuration) {
            return 0; // Cliff not reached
        }

        uint256 elapsed = block.timestamp - schedule.startTime;
        if (elapsed >= schedule.vestDuration) {
            return schedule.total; // Fully vested
        }

        // Linear vesting after cliff
        uint256 vested = (schedule.total * elapsed) / schedule.vestDuration;
        return vested;
    }

    /**
     * @notice Claim vested tokens
     */
    function claimVested() external nonReentrant {
        VestingSchedule storage schedule = vestingSchedules[msg.sender];
        require(schedule.total > 0, "No vesting schedule");

        uint256 vested = vestedAmount(msg.sender);
        uint256 claimable = vested - schedule.released;
        require(claimable > 0, "No tokens to claim");

        schedule.released += claimable;
        _transfer(address(this), msg.sender, claimable);

        emit VestedTokensClaimed(msg.sender, claimable);
    }

    // ============================================================================
    // veCERTUS Locking (Vote-Escrowed CERTUS)
    // ============================================================================

    /**
     * @notice Lock CERTUS to receive veCERTUS
     * @param amount Amount of CERTUS to lock
     * @param lockDuration Duration to lock (1 week to 4 years)
     *
     * @dev veCERTUS calculation:
     * - Lock 1 year: 0.25 veCERTUS per CERTUS
     * - Lock 4 years: 1.0 veCERTUS per CERTUS (max boost)
     */
    function lockCertus(uint256 amount, uint256 lockDuration) external nonReentrant {
        require(amount > 0, "Amount must be > 0");
        require(lockDuration >= MIN_LOCK_DURATION, "Lock too short");
        require(lockDuration <= MAX_LOCK_DURATION, "Lock too long");
        require(veCertusLocks[msg.sender].amount == 0, "Already have active lock");

        // Calculate veCERTUS based on lock duration
        // Linear scaling: 1 week = 0.0048 veCERTUS per CERTUS, 4 years = 1.0 veCERTUS per CERTUS
        uint256 veCertusAmount = (amount * lockDuration) / MAX_LOCK_DURATION;

        // Transfer CERTUS to contract
        _transfer(msg.sender, address(this), amount);

        // Create lock
        veCertusLocks[msg.sender] = VeCertusLock({
            amount: amount,
            unlockTime: block.timestamp + lockDuration,
            veCertusBalance: veCertusAmount,
            lockDuration: lockDuration
        });

        totalVeCertus += veCertusAmount;

        // Update revenue debt for fair distribution
        revenueDebt[msg.sender] = (veCertusAmount * revenueDistribution.accRevenuePerVeCertus) / 1e18;

        emit VeCertusLocked(msg.sender, amount, block.timestamp + lockDuration, veCertusAmount);
    }

    /**
     * @notice Unlock CERTUS after lock period expires
     */
    function unlockCertus() external nonReentrant {
        VeCertusLock storage lock = veCertusLocks[msg.sender];
        require(lock.amount > 0, "No active lock");
        require(block.timestamp >= lock.unlockTime, "Lock period not expired");

        uint256 amount = lock.amount;
        uint256 veCertusAmount = lock.veCertusBalance;

        // Claim any pending revenue before unlock
        _claimRevenue(msg.sender);

        // Delete lock
        delete veCertusLocks[msg.sender];
        totalVeCertus -= veCertusAmount;

        // Return CERTUS
        _transfer(address(this), msg.sender, amount);

        emit VeCertusUnlocked(msg.sender, amount);
    }

    // ============================================================================
    // Revenue Distribution (50% of protocol fees to veCERTUS holders)
    // ============================================================================

    /**
     * @notice Distribute revenue to veCERTUS holders (called by escrow contract)
     * @param amount Amount of USDC to distribute
     */
    function distributeRevenue(uint256 amount) external nonReentrant {
        require(msg.sender == owner(), "Only escrow contract can distribute");
        require(totalVeCertus > 0, "No veCERTUS holders");

        IERC20(revenueToken).safeTransferFrom(msg.sender, address(this), amount);

        // Update accumulated revenue per veCERTUS
        revenueDistribution.accRevenuePerVeCertus += (amount * 1e18) / totalVeCertus;
        revenueDistribution.totalDistributed += amount;
        revenueDistribution.lastDistribution = block.timestamp;

        emit RevenueDistributed(amount, revenueDistribution.accRevenuePerVeCertus);
    }

    /**
     * @notice Calculate pending revenue for a veCERTUS holder
     * @param user Address to check
     * @return Pending USDC revenue
     */
    function pendingRevenue(address user) public view returns (uint256) {
        VeCertusLock memory lock = veCertusLocks[user];
        if (lock.veCertusBalance == 0) return 0;

        uint256 accRevenue = (lock.veCertusBalance * revenueDistribution.accRevenuePerVeCertus) / 1e18;
        return accRevenue - revenueDebt[user];
    }

    /**
     * @notice Claim pending revenue
     */
    function claimRevenue() external nonReentrant {
        _claimRevenue(msg.sender);
    }

    function _claimRevenue(address user) internal {
        uint256 pending = pendingRevenue(user);
        if (pending > 0) {
            revenueDebt[user] = (veCertusLocks[user].veCertusBalance * revenueDistribution.accRevenuePerVeCertus) / 1e18;
            IERC20(revenueToken).safeTransfer(user, pending);
            emit RevenueClaimed(user, pending);
        }
    }

    // ============================================================================
    // Buyback & Burn (30% of protocol fees)
    // ============================================================================

    /**
     * @notice Execute buyback and burn via Uniswap V3
     * @param usdcAmount Amount of USDC to spend on buyback
     * @param minCertusOut Minimum CERTUS to receive (slippage protection)
     */
    function buybackAndBurn(uint256 usdcAmount, uint256 minCertusOut) external nonReentrant onlyOwner {
        require(usdcAmount > 0, "Amount must be > 0");
        require(uniswapRouter != address(0), "Uniswap router not set");

        // Transfer USDC from caller to this contract
        IERC20(revenueToken).safeTransferFrom(msg.sender, address(this), usdcAmount);

        // Approve Uniswap V3 router to spend USDC
        IERC20(revenueToken).approve(uniswapRouter, usdcAmount);

        // Execute swap: USDC -> CERTUS via Uniswap V3
        ISwapRouter.ExactInputSingleParams memory params = ISwapRouter.ExactInputSingleParams({
            tokenIn: revenueToken,
            tokenOut: address(this),
            fee: 3000, // 0.3% pool
            recipient: address(this),
            deadline: block.timestamp,
            amountIn: usdcAmount,
            amountOutMinimum: minCertusOut,
            sqrtPriceLimitX96: 0
        });

        uint256 certusBought = ISwapRouter(uniswapRouter).exactInputSingle(params);
        require(certusBought >= minCertusOut, "Slippage exceeded");

        // Burn purchased CERTUS
        _burn(address(this), certusBought);
        totalBurned += certusBought;

        emit BuybackExecuted(usdcAmount, certusBought, certusBought);
        emit TokensBurned(address(this), certusBought, "Buyback and burn");
    }

    // ============================================================================
    // Utility Mechanism Queries
    // ============================================================================

    /**
     * @notice Get verifier selection boost multiplier (tiered to prevent whale dominance)
     * @param verifier Address to check
     * @return Boost multiplier (1.0x = 100, 1.2x = 120, 1.5x = 150, 2.0x = 200)
     */
    function getVerifierBoost(address verifier) external view returns (uint256) {
        uint256 balance = balanceOf(verifier);
        if (balance >= VERIFIER_BOOST_TIER3) return 200; // 2.0x (max)
        if (balance >= VERIFIER_BOOST_TIER2) return 150; // 1.5x
        if (balance >= VERIFIER_BOOST_TIER1) return 120; // 1.2x
        return 100; // 1.0x (no boost)
    }

    /**
     * @notice Check if executor is eligible for insurance pool
     * @param executor Address to check
     * @return True if executor has staked 50k CERTUS (eligible for 50% slashing coverage)
     *
     * @dev Replaced capital efficiency mechanism with insurance pool model.
     * Insurance pool covers 50% of slashing losses, funded by 10% of protocol fees.
     * Does NOT reduce collateral requirement (all executors pay 2.0x fixed).
     */
    function isInsurancePoolEligible(address executor) external view returns (bool) {
        return balanceOf(executor) >= EXECUTOR_INSURANCE_STAKE;
    }

    /**
     * @notice DEPRECATED: getExecutorCollateralMultiplier
     * @dev Always returns 20000 (2.0x fixed collateral for all executors)
     * Kept for backwards compatibility with external integrations
     */
    function getExecutorCollateralMultiplier(address executor) external pure returns (uint256) {
        return 20000; // 2.0x fixed collateral
    }

    /**
     * @notice Get fee discount for paying in CERTUS
     * @param certusPaymentPercentage Percentage of fee paid in CERTUS (50 or 100)
     * @return Discount in basis points (2500 = 25%, 4000 = 40%)
     */
    function getFeeDiscount(uint256 certusPaymentPercentage) external pure returns (uint256) {
        if (certusPaymentPercentage >= 100) return FEE_DISCOUNT_100PCT_CERTUS; // 40%
        if (certusPaymentPercentage >= 50) return FEE_DISCOUNT_50PCT_CERTUS; // 25%
        return 0;
    }

    /**
     * @notice Burn CERTUS for priority execution
     * @param amount Amount to burn (typically PRIORITY_BURN_AMOUNT)
     */
    function burnForPriority(uint256 amount) external nonReentrant {
        require(amount >= PRIORITY_BURN_AMOUNT, "Insufficient burn amount");
        _burn(msg.sender, amount);
        totalBurned += amount;
        emit TokensBurned(msg.sender, amount, "Priority execution");
    }

    // ============================================================================
    // Volume Tracking (for adaptive fee structure)
    // ============================================================================

    /**
     * @notice Increment monthly payment volume (called by escrow)
     * Tracks payment volume (not job count) to prevent gaming via micro-jobs
     * @param paymentAmount Payment amount in USDC (6 decimals)
     */
    function incrementPaymentVolume(uint256 paymentAmount) external {
        require(msg.sender == owner(), "Only escrow contract");
        require(paymentAmount > 0, "Payment amount must be > 0");

        // Reset volume if new month
        if (block.timestamp > lastVolumeResetTime + 30 days) {
            monthlyPaymentVolume = 0;
            lastVolumeResetTime = block.timestamp;
        }

        monthlyPaymentVolume += paymentAmount;
    }

    /**
     * @notice Get current volume multiplier for fee calculation
     * SECURITY: Based on payment volume (not job count) to prevent micro-job gaming
     * @return Volume multiplier in basis points (10000 = 1.0x, 8000 = 0.8x, etc.)
     *
     * Thresholds (monthly payment volume in USDC):
     * - >$50M: 0.3x multiplier (0.9% effective fee)
     * - $5M-$50M: 0.4x multiplier (1.2% effective fee)
     * - $500k-$5M: 0.6x multiplier (1.8% effective fee)
     * - $50k-$500k: 0.8x multiplier (2.4% effective fee)
     * - <$50k: 1.0x multiplier (3.0% effective fee)
     */
    function getVolumeMultiplier() external view returns (uint256) {
        uint256 volumeUSD = monthlyPaymentVolume; // Already in USDC (6 decimals)

        if (volumeUSD > 50_000_000 * 10**6) return 3000; // 0.3x (>$50M)
        if (volumeUSD > 5_000_000 * 10**6) return 4000; // 0.4x ($5M-$50M)
        if (volumeUSD > 500_000 * 10**6) return 6000; // 0.6x ($500k-$5M)
        if (volumeUSD > 50_000 * 10**6) return 8000; // 0.8x ($50k-$500k)
        return 10000; // 1.0x (<$50k)
    }

    // ============================================================================
    // Governance
    // ============================================================================

    /**
     * @notice Get total voting power (CERTUS + veCERTUS boost)
     * @param voter Address to check
     * @return Total voting power
     */
    function getVotingPower(address voter) external view returns (uint256) {
        uint256 certusBalance = balanceOf(voter);
        uint256 veCertusBalance = veCertusLocks[voter].veCertusBalance;

        // 1 CERTUS = 1 vote, veCERTUS adds boosted voting power
        // veCERTUS with 4-year lock = 2.5x additional voting power
        uint256 veCertusBoost = (veCertusBalance * 250) / 100; // 2.5x

        return certusBalance + veCertusBoost;
    }

    // ============================================================================
    // Liquidity Mining Emissions (Backloaded Curve)
    // ============================================================================

    /**
     * @notice Get monthly CERTUS emission based on backloaded curve
     * @return Monthly emission amount
     *
     * @dev Backloaded emission schedule (reduces early sell pressure):
     * Months 1-6:   100k CERTUS/month = 600k total
     * Months 7-12:  300k CERTUS/month = 1,800k total
     * Months 13-24: 600k CERTUS/month = 7,200k total
     * Months 25-36: 800k CERTUS/month = 9,600k total
     * Months 37-48: 450k CERTUS/month = 5,400k total
     * Total: 24,600k (approx 25M CERTUS over 48 months)
     */
    function getMonthlyEmission() public view returns (uint256) {
        uint256 monthsElapsed = (block.timestamp - deployTime) / 30 days;

        if (monthsElapsed < 6) return 100_000 * 10**18; // Months 1-6
        if (monthsElapsed < 12) return 300_000 * 10**18; // Months 7-12
        if (monthsElapsed < 24) return 600_000 * 10**18; // Months 13-24
        if (monthsElapsed < 36) return 800_000 * 10**18; // Months 25-36
        if (monthsElapsed < 48) return 450_000 * 10**18; // Months 37-48 (FIXED: was 600k)
        return 0; // After 48 months, no more emissions
    }

    /**
     * @notice Calculate dynamic subsidy per verifier
     * @return Subsidy amount in USDC (6 decimals) per verifier
     *
     * @dev Subsidy = max(0, TARGET_INCOME - feeIncome - certusEmissions)
     * Guarantees $200/month minimum income regardless of growth rate.
     */
    function calculateDynamicSubsidy() public view returns (uint256) {
        if (activeVerifierCount == 0) return 0;

        // Fee income per verifier (USDC, 6 decimals)
        uint256 feeIncomePerVerifier = totalMonthlyFees / activeVerifierCount;

        // CERTUS emissions per verifier (static pricing during bootstrap phase)
        // Production: integrate Chainlink price feed for dynamic USD conversion
        uint256 monthlyEmission = getMonthlyEmission();
        uint256 certusPerVerifier = monthlyEmission / activeVerifierCount;
        uint256 certusValueUSD = (certusPerVerifier * 5) / 100; // $0.05 per CERTUS * emissions

        uint256 currentIncome = feeIncomePerVerifier + certusValueUSD;

        if (currentIncome >= TARGET_VERIFIER_INCOME) {
            return 0; // No subsidy needed
        }

        return TARGET_VERIFIER_INCOME - currentIncome;
    }

    /**
     * @notice Update active verifier count (called by escrow contract)
     * @param count Current number of active verifiers
     */
    function updateActiveVerifierCount(uint256 count) external onlyOwner {
        activeVerifierCount = count;
    }

    /**
     * @notice Increment monthly fee total (called by escrow on finalize)
     * @param feeAmount Fee amount in USDC (6 decimals)
     */
    function incrementMonthlyFees(uint256 feeAmount) external onlyOwner {
        // Reset if new month
        if (block.timestamp > lastSubsidyCalculation + 30 days) {
            totalMonthlyFees = 0;
            lastSubsidyCalculation = block.timestamp;
        }

        totalMonthlyFees += feeAmount;
    }

    /**
     * @notice Release liquidity mining rewards to verifiers
     * @param verifier Address to reward
     * @param amount Amount to release
     */
    function releaseMiningRewards(address verifier, uint256 amount) external nonReentrant onlyOwner {
        require(amount > 0, "Amount must be > 0");
        require(balanceOf(address(this)) >= amount, "Insufficient mining reserves");

        _transfer(address(this), verifier, amount);
    }

    // ============================================================================
    // Admin Functions
    // ============================================================================

    function setRevenueToken(address _revenueToken) external onlyOwner {
        require(_revenueToken != address(0), "Invalid token");
        revenueToken = _revenueToken;
    }

    // ============================================================================
    // View Functions
    // ============================================================================

    function circulatingSupply() external view returns (uint256) {
        return TOTAL_SUPPLY - balanceOf(address(this)) - totalBurned;
    }

    function getVeCertusBalance(address user) external view returns (uint256) {
        return veCertusLocks[user].veCertusBalance;
    }

    function getLockInfo(address user) external view returns (
        uint256 amount,
        uint256 unlockTime,
        uint256 veCertusBalance,
        uint256 lockDuration
    ) {
        VeCertusLock memory lock = veCertusLocks[user];
        return (lock.amount, lock.unlockTime, lock.veCertusBalance, lock.lockDuration);
    }
}
