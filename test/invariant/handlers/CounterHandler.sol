// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { CommonBase } from 'forge-std/Base.sol';
import { StdCheats } from 'forge-std/StdCheats.sol';
import { StdUtils } from 'forge-std/StdUtils.sol';
import { Counter } from 'src/Counter.sol';

/**
 * @title CounterHandler
 * @notice Handler contract for Counter invariant testing.
 * @author 0xBerzerk
 *
 * The handler wraps the target contract's functions and constrains inputs to
 * valid ranges. Ghost variables track the expected state so invariants can
 * compare actual vs expected.
 *
 * @dev For an alternative approach to invariant testing, see Recon + Chimera:
 *   - Recon: https://getrecon.xyz — automated invariant test generation
 *   - Chimera: https://github.com/Recon-Fuzz/chimera — framework that makes
 *     handler contracts compatible with both Foundry and Echidna/Medusa.
 *     Chimera provides a unified interface so you write handlers once and
 *     run them across multiple fuzzing engines.
 */
contract CounterHandler is CommonBase, StdCheats, StdUtils {
  /* ////////////////////////////////////////////////
                        State
  ////////////////////////////////////////////////*/

  Counter public immutable i_counter;

  /**
   * @notice Ghost variable tracking the expected value of counter.getNumber().
   */
  uint256 public ghost_number;

  /**
   * @notice Call counters for debugging invariant failures.
   */
  uint256 public calls_setNumber;
  uint256 public calls_increment;

  /* ////////////////////////////////////////////////
                      Constructor
  ////////////////////////////////////////////////*/

  constructor(Counter _counter) {
    i_counter = _counter;
  }

  /* ////////////////////////////////////////////////
                    Handler Actions
  ////////////////////////////////////////////////*/

  /**
   * @notice Bounded setNumber — constrains input to the valid range [0, uint128.max].
   * @param _newNumber The fuzzed input, bounded to [0, uint128.max] before calling.
   */
  function setNumber(uint256 _newNumber) external {
    _newNumber = bound(_newNumber, 0, type(uint128).max);

    i_counter.setNumber(_newNumber);

    ghost_number = _newNumber;
    ++calls_setNumber;
  }

  /**
   * @notice Increment — skips if number >= uint128.max to avoid overflow
   *         that would break subsequent setNumber calls.
   */
  function increment() external {
    // Skip if incrementing would push number beyond what setNumber accepts
    if (i_counter.getNumber() > type(uint128).max - 1) return;

    i_counter.increment();

    ++ghost_number;
    ++calls_increment;
  }
}
