// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { Counter } from 'src/Counter.sol';
import { BaseTest } from 'test/Base.t.sol';

/**
 * @title CounterUnitTest
 * @notice Base contract for Counter unit tests.
 * @author 0xBerzerk
 *
 * Deploys Counter directly (no deploy script). Unit tests must be stateless
 * and mock any external dependencies via vm.mockCall() or vm.mockFunction().
 *
 * @dev Example mocking pattern (for contracts with dependencies):
 *
 *   // Mock an external call to oracle.getPrice(token) returning 1e18
 *   vm.mockCall(
 *     address(oracle),
 *     abi.encodeCall(IOracle.getPrice, (token)),
 *     abi.encode(1e18)
 *   );
 *
 *   // Or mock a function to redirect to a different implementation
 *   vm.mockFunction(
 *     address(realContract),
 *     address(mockContract),
 *     abi.encodeWithSelector(IContract.someFunction.selector)
 *   );
 */
abstract contract CounterUnitTest is BaseTest {
  Counter internal counter;

  function setUp() public virtual override {
    super.setUp();

    // Deploy the contract under test directly — no deploy script in unit tests
    counter = new Counter();
    vm.label(address(counter), 'Counter');
  }
}
