// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.20 <0.9.0;

/**
 * @title ICounter
 * @notice Interface for the Counter contract.
 * @author 0xBerzerk
 */
interface ICounter {
  /* ////////////////////////////////////////////////
                      Events
  ////////////////////////////////////////////////*/

  /**
   * @notice Emitted when `s_number` is updated via setNumber.
   * @param _newNumber The new value stored.
   */
  event NumberSet(uint256 indexed _newNumber);

  /**
   * @notice Emitted when `s_number` is incremented.
   * @param _newNumber The value after incrementing.
   */
  event Incremented(uint256 indexed _newNumber);

  /* ////////////////////////////////////////////////
                      Errors
  ////////////////////////////////////////////////*/

  /**
   * @notice Reverts when `_newNumber` exceeds the uint128 maximum.
   */
  error Counter_NumberTooLarge();

  /* ////////////////////////////////////////////////
                      Functions
  ////////////////////////////////////////////////*/

  /**
   * @notice Sets `s_number` to a new value.
   * @param _newNumber The value to store. Must be <= uint128 max.
   */
  function setNumber(uint256 _newNumber) external;

  /**
   * @notice Increments `s_number` by one.
   */
  function increment() external;

  /**
   * @notice Returns the current stored value.
   * @return number_ The current value.
   */
  function getNumber() external view returns (uint256 number_);
}
