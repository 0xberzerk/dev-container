// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { Test } from 'forge-std/Test.sol';

/**
 * @title BaseTest
 * @notice Global base test contract. All test contracts inherit from this.
 * @author 0xBerzerk
 * @dev Responsibilities:
 *   - Create named test actors via makeAddr() for readable traces
 *   - Fund actors with ETH
 *   - Provide a virtual setUp() that child contracts extend
 */
abstract contract BaseTest is Test {
  /* ////////////////////////////////////////////////
                        Actors
  ////////////////////////////////////////////////*/

  address internal alice;
  address internal bob;

  /* ////////////////////////////////////////////////
                        Set up
  ////////////////////////////////////////////////*/

  function setUp() public virtual {
    // Create named actors — traces show "alice" / "bob" instead of raw addresses
    alice = makeAddr('alice');
    bob = makeAddr('bob');

    // Fund actors
    vm.deal(alice, 100 ether);
    vm.deal(bob, 100 ether);
  }
}
