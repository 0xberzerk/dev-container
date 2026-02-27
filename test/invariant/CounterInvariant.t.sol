// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { console } from 'forge-std/console.sol';
import { Counter } from 'src/Counter.sol';
import { BaseTest } from 'test/Base.t.sol';
import { CounterHandler } from 'test/invariant/handlers/CounterHandler.sol';

/**
 * @title CounterInvariantTest
 * @notice Invariant tests for Counter.
 * @author 0xBerzerk
 *
 * Invariant tests (also called stateful fuzz tests) call random sequences of
 * handler functions and assert properties that must hold after every call.
 *
 * @dev Foundry configuration in foundry.toml:
 *   [invariant]
 *   runs = 256
 *   depth = 100
 *
 * Alternative: Recon + Chimera
 *   For more advanced invariant testing with coverage-guided fuzzing,
 *   consider using Recon (https://getrecon.xyz) with the Chimera framework.
 *   Chimera allows writing handler contracts that work with both Foundry's
 *   built-in fuzzer AND Echidna/Medusa, giving you multiple fuzzing engines
 *   from the same test suite. See: https://github.com/Recon-Fuzz/chimera
 */
contract CounterInvariantTest is BaseTest {
  Counter internal counter;
  CounterHandler internal handler;

  function setUp() public override {
    super.setUp();

    // Deploy the system
    counter = new Counter();
    vm.label(address(counter), 'Counter');

    // Deploy the handler and point it at the counter
    handler = new CounterHandler(counter);
    vm.label(address(handler), 'CounterHandler');

    // Tell Foundry to only call functions on the handler, not the counter directly.
    // This ensures all inputs are properly bounded.
    targetContract(address(handler));
  }

  /* ////////////////////////////////////////////////
                      Invariants
  ////////////////////////////////////////////////*/

  /**
   * @notice The counter's number must never exceed uint128.max, because
   *         setNumber reverts on larger values and increment is guarded.
   */
  function invariant_numberNeverExceedsUint128Max() public view {
    assertLe(counter.getNumber(), type(uint128).max);
  }

  /**
   * @notice The counter's actual state must match the handler's ghost tracking.
   *         This catches any desync between expected and actual behavior.
   */
  function invariant_numberMatchesGhost() public view {
    assertEq(counter.getNumber(), handler.ghost_number());
  }

  /**
   * @dev Log call distribution after each invariant run for debugging.
   */
  function afterInvariant() public view {
    console.log('setNumber calls:', handler.calls_setNumber());
    console.log('increment calls:', handler.calls_increment());
  }
}
