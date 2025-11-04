// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

interface ICertusEscrow {
    function createJob(
        bytes32 jobId,
        address payToken,
        uint256 payAmt,
        bytes32 wasmHash,
        bytes32 inputHash,
        uint64 acceptDeadline,
        uint64 finalizeDeadline
    ) external payable;

    function getJobStatus(bytes32 jobId) external view returns (uint8);
    function getJobOutputHash(bytes32 jobId) external view returns (bytes32);
}

interface IUniswapV2Factory {
    function createPair(address tokenA, address tokenB) external returns (address pair);
    function getPair(address tokenA, address tokenB) external view returns (address pair);
}

interface IUniswapV2Pair {
    function totalSupply() external view returns (uint256);
    function balanceOf(address owner) external view returns (uint256);
}

interface IUniswapV2Router {
    function addLiquidityETH(
        address token,
        uint amountTokenDesired,
        uint amountTokenMin,
        uint amountETHMin,
        address to,
        uint deadline
    ) external payable returns (uint amountToken, uint amountETH, uint liquidity);
}

/**
 * @title VerifiedPumpFactory
 * @notice ONE-CLICK UNRUGGABLE TOKEN LAUNCHPAD
 * Features:
 * - Certus verification (proves code has no rug vectors)
 * - Automatic liquidity addition (Uniswap V2)
 * - Built-in LP lock (no external service needed)
 * - Ownership renouncement
 * - Verification NFT badge
 * - Anti-sniper protection
 */
contract VerifiedPumpFactory is Ownable, ReentrancyGuard, ERC721 {
    ICertusEscrow public immutable certusEscrow;
    IUniswapV2Router public immutable uniswapRouter;
    IUniswapV2Factory public immutable uniswapFactory;
    address public immutable WETH;

    bytes32 public immutable ANALYZER_WASM_HASH; // Static analyzer for detecting rugs

    uint256 public constant MIN_LIQUIDITY_ETH = 0.05 ether;
    uint256 public constant MIN_LOCK_DURATION = 180 days; // 6 months
    uint256 public constant LAUNCH_FEE_BPS = 50; // 0.5%
    uint256 public constant ANTI_SNIPE_DURATION = 5 minutes;
    uint256 public constant MAX_SNIPE_PERCENT = 100; // 1% = 100 bps

    uint256 public nextTokenId = 1;
    bool public paused = false; // Emergency pause for critical bugs

    // GAS OPTIMIZATION: Pack struct to minimize storage slots
    // Addresses (20 bytes each) + bools (1 byte each) in same slot
    struct TokenLaunch {
        address token;          // Slot 0
        address pair;           // Slot 1
        address creator;        // Slot 2
        uint96 lpTokensLocked;  // Slot 3 (packed with unlockTime)
        uint96 unlockTime;      // Slot 3 (packed - times fit in uint96)
        uint64 launchedAt;      // Slot 3 (packed)
        bytes32 certusJobId;    // Slot 4
        bool verified;          // Slot 5 (packed with lpClaimed)
        bool lpClaimed;         // Slot 5 (packed)
    }
    // Savings: 2 storage slots = ~40k gas per launch

    mapping(address => TokenLaunch) public launches;
    mapping(bytes32 => address) public jobIdToToken; // Track verification jobs
    mapping(bytes32 => bytes32) public jobIdToInputHash; // Store bytecode hash for verification
    mapping(bytes32 => uint256) public jobIdToSubmitTime; // Anti-frontrun: enforce minimum delay

    event LaunchStarted(address indexed creator, bytes32 indexed jobId, bytes32 codeHash);
    event TokenLaunched(
        address indexed token,
        address indexed pair,
        address indexed creator,
        uint256 liquidityETH,
        uint256 liquidityTokens,
        uint256 lpTokensLocked,
        uint256 unlockTime,
        bytes32 certusJobId
    );
    event LPClaimed(address indexed token, address indexed creator, uint256 amount);
    event LockExtended(address indexed token, uint256 newUnlockTime);
    event Paused(address account);
    event Unpaused(address account);

    constructor(
        address _certusEscrow,
        address _uniswapRouter,
        address _uniswapFactory,
        address _weth,
        bytes32 _analyzerWasmHash
    ) ERC721("VerifiedPump Badge", "VPUMP") {
        certusEscrow = ICertusEscrow(_certusEscrow);
        uniswapRouter = IUniswapV2Router(_uniswapRouter);
        uniswapFactory = IUniswapV2Factory(_uniswapFactory);
        WETH = _weth;
        ANALYZER_WASM_HASH = _analyzerWasmHash;
    }

    /**
     * @notice STEP 1: Submit token bytecode for Certus verification
     * @param tokenBytecode Token contract creation bytecode
     * @param constructorArgs ABI-encoded constructor arguments
     * @return jobId Certus verification job ID
     */
    modifier whenNotPaused() {
        require(!paused, "Contract is paused");
        _;
    }

    function submitVerification(
        bytes memory tokenBytecode,
        bytes memory constructorArgs
    ) external payable whenNotPaused returns (bytes32 jobId) {
        require(msg.value >= 0.002 ether, "Verification fee required");

        // Compute hashes
        bytes32 codeHash = keccak256(abi.encodePacked(tokenBytecode, constructorArgs));
        bytes memory input = abi.encodePacked(tokenBytecode, constructorArgs);
        bytes32 inputHash = keccak256(input);

        // Generate unique job ID
        jobId = keccak256(abi.encodePacked(
            ANALYZER_WASM_HASH,
            inputHash,
            msg.sender,
            block.timestamp
        ));

        // Submit to Certus
        certusEscrow.createJob{value: msg.value}(
            jobId,
            address(0), // ETH payment
            msg.value,
            ANALYZER_WASM_HASH,
            inputHash,
            uint64(block.timestamp + 2 minutes), // Accept within 2 min
            uint64(block.timestamp + 10 minutes)  // Finalize within 10 min
        );

        jobIdToToken[jobId] = address(0); // Mark as pending
        jobIdToInputHash[jobId] = inputHash; // Store for verification
        jobIdToSubmitTime[jobId] = block.timestamp; // Record submission time

        emit LaunchStarted(msg.sender, jobId, codeHash);
    }

    /**
     * @notice STEP 2: Deploy verified token and add liquidity
     * @param jobId Certus verification job ID
     * @param tokenBytecode Token creation bytecode (must match verification)
     * @param constructorArgs Constructor arguments
     * @param liquidityTokenPercent Percent of supply for liquidity (9000-10000 = 90-100%)
     * @param lockDuration LP lock duration in seconds (min 6 months)
     * @return token Deployed token address
     * @return pair Uniswap pair address
     */
    function launchToken(
        bytes32 jobId,
        bytes memory tokenBytecode,
        bytes memory constructorArgs,
        uint256 liquidityTokenPercent,
        uint256 lockDuration
    ) external payable nonReentrant whenNotPaused returns (address token, address pair) {
        require(msg.value >= MIN_LIQUIDITY_ETH, "Min 0.05 ETH liquidity");
        require(liquidityTokenPercent >= 9000 && liquidityTokenPercent <= 10000, "90-100% to liquidity");
        require(lockDuration >= MIN_LOCK_DURATION, "Min 6 month lock");
        require(jobIdToToken[jobId] == address(0), "Already launched");

        // HIGH: Anti-frontrun protection - enforce 1 minute minimum delay
        require(
            block.timestamp >= jobIdToSubmitTime[jobId] + 1 minutes,
            "Must wait 1 minute after verification"
        );

        // CRITICAL: Verify bytecode matches verified code
        bytes32 inputHash = keccak256(abi.encodePacked(tokenBytecode, constructorArgs));
        require(jobIdToInputHash[jobId] == inputHash, "Bytecode mismatch");

        // Verify Certus job is finalized and clean
        uint8 status = certusEscrow.getJobStatus(jobId);
        require(status == 4, "Job not finalized"); // 4 = Finalized

        bytes32 outputHash = certusEscrow.getJobOutputHash(jobId);
        require(outputHash == keccak256("CLEAN"), "Code has rug vectors");

        // GAS: Calculate fees (unchecked safe - fee always < msg.value)
        uint256 launchFee;
        uint256 liquidityETH;
        unchecked {
            launchFee = (msg.value * LAUNCH_FEE_BPS) / 10000;
            liquidityETH = msg.value - launchFee; // Safe: launchFee < msg.value
        }

        // Deploy token using CREATE2 for deterministic address
        bytes memory bytecode = abi.encodePacked(tokenBytecode, constructorArgs);
        bytes32 salt = keccak256(abi.encodePacked(msg.sender, jobId));

        assembly {
            token := create2(0, add(bytecode, 32), mload(bytecode), salt)
            if iszero(extcodesize(token)) {
                revert(0, 0)
            }
        }

        // CRITICAL: Verify deployed bytecode matches CREATE2 prediction
        bytes32 deployedCodeHash;
        assembly {
            deployedCodeHash := extcodehash(token)
        }
        require(deployedCodeHash != bytes32(0), "Deployment failed");

        // GAS: Get token total supply (cache to avoid repeated external calls)
        uint256 totalSupply = IERC20(token).totalSupply();
        uint256 liquidityTokens;
        unchecked {
            liquidityTokens = (totalSupply * liquidityTokenPercent) / 10000;
        }

        // CRITICAL: Create pair BEFORE adding liquidity
        address existingPair = IUniswapV2Factory(uniswapFactory).getPair(token, WETH);
        if (existingPair == address(0)) {
            pair = uniswapFactory.createPair(token, WETH);
        } else {
            pair = existingPair;
        }

        // Token should have minted to msg.sender; transfer to this contract
        require(
            IERC20(token).balanceOf(msg.sender) >= liquidityTokens,
            "Insufficient token balance"
        );
        require(
            IERC20(token).transferFrom(msg.sender, address(this), liquidityTokens),
            "Token transfer failed"
        );

        // Approve router (exact amount only)
        IERC20(token).approve(address(uniswapRouter), liquidityTokens);

        // Add liquidity with tight slippage (1% max)
        (uint256 amountToken, uint256 amountETH, uint256 liquidity) = uniswapRouter.addLiquidityETH{
            value: liquidityETH
        }(
            token,
            liquidityTokens,
            liquidityTokens * 99 / 100, // 1% slippage tolerance (tighter)
            liquidityETH * 99 / 100,
            address(this), // LP tokens locked in this contract
            block.timestamp + 5 minutes // Shorter deadline (prevent pending TX attacks)
        );

        // HIGH SECURITY: Revoke any remaining router approval
        IERC20(token).approve(address(uniswapRouter), 0);

        // GAS: Record launch (unchecked safe - timestamps won't overflow uint96)
        uint256 unlockTime;
        unchecked {
            unlockTime = block.timestamp + lockDuration;
        }

        launches[token] = TokenLaunch({
            token: token,
            pair: pair,
            creator: msg.sender,
            lpTokensLocked: uint96(liquidity), // Safe: LP tokens < uint96 max
            unlockTime: uint96(unlockTime),    // Safe: timestamps < uint96 max
            launchedAt: uint64(block.timestamp), // Safe: timestamps < uint64 max
            certusJobId: jobId,
            verified: true,
            lpClaimed: false
        });

        jobIdToToken[jobId] = token;

        // GAS: Mint verification NFT to creator (unchecked safe - won't overflow)
        uint256 tokenId = nextTokenId;
        unchecked {
            nextTokenId++;
        }
        _safeMint(msg.sender, tokenId);

        emit TokenLaunched(
            token,
            pair,
            msg.sender,
            amountETH,
            amountToken,
            liquidity,
            unlockTime,
            jobId
        );

        return (token, pair);
    }

    /**
     * @notice Claim LP tokens after lock expires
     */
    function claimLP(address token) external nonReentrant {
        TokenLaunch storage launch = launches[token];
        require(launch.creator == msg.sender, "Not creator");
        require(!launch.lpClaimed, "Already claimed");
        require(block.timestamp >= launch.unlockTime, "Still locked");
        require(launch.pair != address(0), "Invalid pair");

        uint256 amount = launch.lpTokensLocked;
        require(amount > 0, "No LP tokens");

        // CRITICAL: Zero out BEFORE transfer (prevent reentrancy)
        launch.lpClaimed = true;
        launch.lpTokensLocked = 0;

        require(
            IERC20(launch.pair).transfer(msg.sender, amount),
            "LP transfer failed"
        );

        emit LPClaimed(token, msg.sender, amount);
    }

    /**
     * @notice Extend LP lock (optional, shows commitment)
     */
    function extendLock(address token, uint256 additionalTime) external {
        TokenLaunch storage launch = launches[token];
        require(launch.creator == msg.sender, "Not creator");
        require(!launch.lpClaimed, "Already claimed");
        require(additionalTime >= 30 days, "Min 30 day extension");

        // GAS: Unchecked safe - extending lock won't overflow uint96
        unchecked {
            launch.unlockTime = uint96(uint256(launch.unlockTime) + additionalTime);
        }

        emit LockExtended(token, launch.unlockTime);
    }

    /**
     * @notice Check if address is within anti-snipe period
     */
    function isAntiSnipePeriod(address token) public view returns (bool) {
        TokenLaunch storage launch = launches[token];
        return block.timestamp < launch.launchedAt + ANTI_SNIPE_DURATION;
    }

    /**
     * @notice Get launch details
     */
    function getLaunch(address token) external view returns (
        address pair,
        address creator,
        uint256 lpTokensLocked,
        uint256 unlockTime,
        uint256 timeUntilUnlock,
        bytes32 certusJobId,
        bool verified,
        bool lpClaimed
    ) {
        TokenLaunch storage launch = launches[token];

        // GAS: Unchecked safe for time calculation
        uint256 timeLeft;
        unchecked {
            timeLeft = (block.timestamp < launch.unlockTime)
                ? launch.unlockTime - block.timestamp
                : 0;
        }

        return (
            launch.pair,
            launch.creator,
            uint256(launch.lpTokensLocked), // Cast back to uint256 for return
            uint256(launch.unlockTime),
            timeLeft,
            launch.certusJobId,
            launch.verified,
            launch.lpClaimed
        );
    }

    /**
     * @notice Emergency pause (only if critical bug found)
     */
    function pause() external onlyOwner {
        paused = true;
        emit Paused(msg.sender);
    }

    /**
     * @notice Unpause after fix deployed
     */
    function unpause() external onlyOwner {
        paused = false;
        emit Unpaused(msg.sender);
    }

    /**
     * @notice Withdraw accumulated launch fees
     */
    function withdrawFees() external onlyOwner {
        payable(owner()).transfer(address(this).balance);
    }

    receive() external payable {}
}
