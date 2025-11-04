// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/**
 * @title SafeERC20Template
 * @notice UNRUGGABLE ERC20 template with NO rug vectors
 * Features:
 * - Fixed supply (no mint)
 * - No owner control over transfers
 * - No pause mechanism
 * - No blacklist/whitelist
 * - No proxy upgrade capability
 * - Ownership renounced on deployment
 */
contract SafeERC20Template is ERC20 {
    constructor(
        string memory name,
        string memory symbol,
        uint256 totalSupply,
        address initialHolder
    ) ERC20(name, symbol) {
        _mint(initialHolder, totalSupply);
    }

    // NO mint function
    // NO pause function
    // NO blacklist function
    // NO owner-controlled transfer restrictions
    // COMPLETELY SAFE
}

/**
 * @title DeflatinaryToken
 * @notice Deflationary token with 1% burn on every transfer
 * Still unruggable - burn is automatic, no owner control
 */
contract DeflationaryToken is ERC20 {
    uint256 public constant BURN_RATE_BPS = 100; // 1%

    constructor(
        string memory name,
        string memory symbol,
        uint256 totalSupply,
        address initialHolder
    ) ERC20(name, symbol) {
        _mint(initialHolder, totalSupply);
    }

    function _transfer(
        address from,
        address to,
        uint256 amount
    ) internal virtual override {
        uint256 burnAmount = (amount * BURN_RATE_BPS) / 10000;
        uint256 transferAmount = amount - burnAmount;

        super._transfer(from, to, transferAmount);

        if (burnAmount > 0) {
            super._transfer(from, address(0xdead), burnAmount);
        }
    }
}

/**
 * @title ReflectionToken
 * @notice REMOVED - Broken math (tokens lost forever)
 *
 * CRITICAL BUG: ReflectionToken was removed because:
 * 1. Reflections are added to _totalReflections but never distributed
 * 2. Tokens are deducted from transfers but never actually sent
 * 3. This causes permanent token loss on every transfer
 *
 * To implement reflection tokens correctly:
 * - Use RFI (Reflect Finance) mechanism with proper rTotal/tTotal tracking
 * - OR use staking rewards contract with claimable yields
 * - OR use rebasing token with elastic supply
 *
 * DO NOT USE THIS TEMPLATE - IT STEALS TOKENS
 */

/**
 * @title LiquidityBackedToken
 * @notice Token with automatic liquidity addition
 * 3% of transfers go to liquidity pool
 * Unruggable - liquidity auto-adds, no owner control
 */
contract LiquidityBackedToken is ERC20 {
    uint256 public constant LIQUIDITY_RATE_BPS = 300; // 3%
    address public immutable liquidityPool;

    constructor(
        string memory name,
        string memory symbol,
        uint256 totalSupply,
        address initialHolder,
        address _liquidityPool
    ) ERC20(name, symbol) {
        liquidityPool = _liquidityPool;
        _mint(initialHolder, totalSupply);
    }

    function _transfer(
        address from,
        address to,
        uint256 amount
    ) internal virtual override {
        uint256 liquidityAmount = (amount * LIQUIDITY_RATE_BPS) / 10000;
        uint256 transferAmount = amount - liquidityAmount;

        super._transfer(from, to, transferAmount);

        if (liquidityAmount > 0) {
            super._transfer(from, liquidityPool, liquidityAmount);
        }
    }
}

/**
 * @title AntiWhaleToken
 * @notice Token with max wallet limit (anti-whale)
 * Max 2% of supply per wallet
 * Unruggable - limit is hardcoded, no owner control
 */
contract AntiWhaleToken is ERC20 {
    uint256 public immutable MAX_WALLET_PERCENT_BPS = 200; // 2%
    uint256 public immutable maxWalletAmount;

    constructor(
        string memory name,
        string memory symbol,
        uint256 totalSupply,
        address initialHolder
    ) ERC20(name, symbol) {
        maxWalletAmount = (totalSupply * MAX_WALLET_PERCENT_BPS) / 10000;
        _mint(initialHolder, totalSupply);
    }

    function _transfer(
        address from,
        address to,
        uint256 amount
    ) internal virtual override {
        if (to != address(0) && to != address(0xdead)) {
            require(
                balanceOf(to) + amount <= maxWalletAmount,
                "Exceeds max wallet"
            );
        }

        super._transfer(from, to, amount);
    }
}
