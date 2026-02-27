// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { ICounter } from 'src/interfaces/ICounter.sol';
import { CounterUnitTest } from 'test/unit/Counter/Counter.t.sol';

/**
 * @title IncrementUnitTest
 * @notice Unit tests for Counter.increment().
 * @author 0xBerzerk
 */
contract IncrementUnitTest is CounterUnitTest {
  modifier givenTheNumberIsZero() {
    assertEq(counter.getNumber(), 0);
    _;
  }

  modifier givenTheNumberIsNotZero(uint256 _startingNumber) {
    _startingNumber = bound(_startingNumber, 1, type(uint128).max - 1);
    counter.setNumber(_startingNumber);
    _;
  }

  function test_GivenTheNumberIsZero() external givenTheNumberIsZero {
    // it should set number to one.
    // it should emit an Incremented event.
    vm.expectEmit(address(counter));
    emit ICounter.Incremented(1);
    counter.increment();
    assertEq(counter.getNumber(), 1);
  }

  function test_GivenTheNumberIsNotZero(uint256 _startingNumber) external givenTheNumberIsNotZero(_startingNumber) {
    // it should increment by one.
    // it should emit an Incremented event.
    uint256 numberBefore = counter.getNumber();
    vm.expectEmit(address(counter));
    emit ICounter.Incremented(numberBefore + 1);
    counter.increment();
    assertEq(counter.getNumber(), numberBefore + 1);
  }
}
