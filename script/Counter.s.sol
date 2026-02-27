// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.30;

import { Script } from 'forge-std/Script.sol';
import { Counter } from 'src/Counter.sol';

/**
 * @title CounterScript
 * @notice Deployment script for the Counter contract.
 * @author 0xBerzerk
 */
contract CounterScript is Script {
  Counter public counter;

  function setUp() public { }

  function run() public {
    vm.startBroadcast();

    counter = new Counter();

    vm.stopBroadcast();
  }
}
