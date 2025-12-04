// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

interface ICasinoGame {
    struct GameConfig {
        address treasury;
        uint256 minBet;
        uint256 maxBet;
        uint16 houseEdgeBps; // Basis points (e.g., 250 = 2.5%)
        bool paused;
    }

    struct GameState {
        address player;
        uint256 betAmount;
        uint256 gameId;
        uint256 timestamp;
        bool settled;
        uint256 result;
        uint256 payout;
    }

    event GamePlayed(
        address indexed player,
        uint256 indexed gameId,
        uint256 betAmount,
        uint256 result,
        uint256 payout
    );

    function initialize(
        address treasury,
        uint256 minBet,
        uint256 maxBet,
        uint16 houseEdgeBps
    ) external;

    function pause() external;
    function unpause() external;
}

