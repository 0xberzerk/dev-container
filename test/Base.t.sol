// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { Test } from 'forge-std/Test.sol';

/**
 * @title BaseTest
 * @notice Global base test contract for security auditing.
 * @author 0xBerzerk
 * @dev Provides common actors for exploit scenarios:
 *   - attacker: adversarial actor (untrusted)
 *   - victim: target of the exploit
 *   - admin: protocol administrator (privileged)
 *   - protocolOwner: protocol owner (highest privilege)
 *   All actors funded with 100 ETH by default.
 */
abstract contract BaseTest is Test {
  /* ////////////////////////////////////////////////
                      Actors
  ////////////////////////////////////////////////*/

  address internal attacker;
  address internal victim;
  address internal admin;
  address internal protocolOwner;

  /* ////////////////////////////////////////////////
                      Set up
  ////////////////////////////////////////////////*/

  function setUp() public virtual {
    attacker = makeAddr('attacker');
    victim = makeAddr('victim');
    admin = makeAddr('admin');
    protocolOwner = makeAddr('protocolOwner');

    vm.deal(attacker, 100 ether);
    vm.deal(victim, 100 ether);
    vm.deal(admin, 100 ether);
    vm.deal(protocolOwner, 100 ether);
  }
}
