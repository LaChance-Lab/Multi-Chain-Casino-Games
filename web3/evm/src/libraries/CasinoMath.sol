// SPDX-License-Identifier: MIT
pragma solidity ^0.8.23;

library CasinoMath {
    error InvalidBetAmount();
    error BetTooLow();
    error BetTooHigh();
    error MathOverflow();

    function calculatePayout(
        uint256 betAmount,
        uint256 multiplierBps,
        uint16 houseEdgeBps
    ) internal pure returns (uint256) {
        uint256 payout = (betAmount * multiplierBps) / 10000;
        uint256 houseEdge = (payout * houseEdgeBps) / 10000;
        return payout - houseEdge;
    }

    function validateBet(
        uint256 betAmount,
        uint256 minBet,
        uint256 maxBet
    ) internal pure {
        if (betAmount < minBet) revert BetTooLow();
        if (betAmount > maxBet) revert BetTooHigh();
    }

    function generateRandom(uint256 seed, uint256 max) internal view returns (uint256) {
        return uint256(keccak256(abi.encodePacked(block.timestamp, block.prevrandao, seed))) % (max + 1);
    }
}

