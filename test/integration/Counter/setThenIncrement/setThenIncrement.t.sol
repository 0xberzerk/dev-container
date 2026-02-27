// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { CounterIntegrationTest } from 'test/integration/Counter/Counter.t.sol';

/**
 * @title SetThenIncrementIntegrationTest
 * @notice Integration tests for Counter.setNumber() followed by Counter.increment().
 * @author 0xBerzerk
 */
contract SetThenIncrementIntegrationTest is CounterIntegrationTest {
  modifier givenTheInitialNumberIsSetViaSetNumber(uint256 _initialNumber) {
    _initialNumber = bound(_initialNumber, 0, type(uint128).max - type(uint8).max);
    counter.setNumber(_initialNumber);
    _;
  }

  function test_WhenIncrementIsCalledAfterSetting(
    uint256 _initialNumber,
    uint8 _incrementCount
  ) external givenTheInitialNumberIsSetViaSetNumber(_initialNumber) {
    // it should reflect both operations.
    // it should maintain correct accumulated state.
    uint256 numberAfterSet = counter.getNumber();
    _incrementCount = uint8(bound(_incrementCount, 1, type(uint8).max));

    for (uint256 i = 0; i < _incrementCount; ++i) {
      counter.increment();
    }

    assertEq(counter.getNumber(), numberAfterSet + _incrementCount);
  }
}
