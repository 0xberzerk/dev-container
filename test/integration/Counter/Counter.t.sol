// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

import { CounterScript } from 'script/Counter.s.sol';
import { Counter } from 'src/Counter.sol';
import { BaseTest } from 'test/Base.t.sol';

/**
 * @title CounterIntegrationTest
 * @notice Base contract for Counter integration tests.
 * @author 0xBerzerk
 *
 * Integration tests run on a fork of a live network at a pinned block number.
 * Contracts are deployed via the same deploy scripts used in production, ensuring
 * the deployment process itself is tested.
 *
 * @dev Setup:
 *   1. Set FORK_RPC_URL in your .env (e.g. an Alchemy/Infura endpoint)
 *   2. Run with: FOUNDRY_PROFILE=integration forge test --match-path 'test/integration/**'
 *
 * For fork testing docs, see: https://book.getfoundry.sh/forge/fork-testing
 */
abstract contract CounterIntegrationTest is BaseTest {
  Counter internal counter;

  /**
   * @notice Pinned block number for deterministic fork state.
   * @dev Update this when you need fresh chain state.
   */
  uint256 internal constant FORK_BLOCK = 21_000_000;

  function setUp() public virtual override {
    super.setUp();

    // Fork the network at a static block for deterministic tests.
    // Uses FORK_RPC_URL env var — if not set, tests in this directory are skipped
    // by the no_match_path in foundry.toml [profile.default].
    string memory rpcUrl = vm.envOr('FORK_RPC_URL', string(''));
    if (bytes(rpcUrl).length > 0) {
      vm.createSelectFork(rpcUrl, FORK_BLOCK);
    }

    // Deploy via the production deploy script
    CounterScript deployScript = new CounterScript();
    deployScript.setUp();
    deployScript.run();

    // Retrieve the deployed contract from the script
    counter = deployScript.counter();
    vm.label(address(counter), 'Counter');
  }
}
