// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.30;

import { ICounter } from 'src/interfaces/ICounter.sol';

/**
 * @title Counter
 * @notice A simple counter contract for demonstrating testing patterns.
 * @author 0xBerzerk
 */
contract Counter is ICounter {
  /* ////////////////////////////////////////////////
                        State
  ////////////////////////////////////////////////*/
  /**
   * @notice The current stored value.
   */
  uint256 internal s_number;

  /* ////////////////////////////////////////////////
                       Functions
  ////////////////////////////////////////////////*/

  /**
   * @inheritdoc ICounter
   */
  function getNumber() external view returns (uint256 number_) {
    number_ = s_number;
  }

  /**
   * @inheritdoc ICounter
   */
  function setNumber(uint256 _newNumber) public {
    if (_newNumber > type(uint128).max) revert Counter_NumberTooLarge();
    s_number = _newNumber;
    emit NumberSet(_newNumber);
  }

  /**
   * @inheritdoc ICounter
   */
  function increment() public {
    ++s_number;
    emit Incremented(s_number);
  }
}
