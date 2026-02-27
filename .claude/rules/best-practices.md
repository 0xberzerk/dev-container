# Solidity Best Practices

## Gas Optimizations

- **Errors:** custom errors only, never `require("string")`
- **Modifiers:** to avoid duplicated checks only. Never use for single application
- **Increment:** always use `++i` instead of `i++` or `variable = variable + 1`
- **Storage packing:** pack smaller variables together in structs when possible
- **Bytes:** prefer `bytes32` over `string` for fixed-length data
- **Cache storage reads:** read storage once into a local variable, reuse the copy — avoid redundant SLOADs
- **Structs handling:** Avoid copying whole structures into memory when few struct components are used, use storage pointer instead
- **Immutable/constant:** use `immutable` for values set once at construction, `constant` for compile-time values

## Magic Numbers

- Never use raw numeric literals in logic — define named `constant` or `immutable` variables
- Acceptable exceptions: `0`, `1`, common bases like `1e18`, and type bounds like `type(uint128).max`

## Security Patterns

- Check-Effects-Interactions pattern for external calls
- Use `ReentrancyGuard` or equivalent when calling untrusted contracts
- Validate all external inputs at system boundaries
- Be aware of weird erc20 tokens and its unique behaviors, use `SafeERC20` for broader handling
- Prefer pull over push for ETH transfers
- Never trust return values from arbitrary external calls without validation
- Set gas limits on external calls to untrusted contracts
- Handle `address(0)` checks before external calls

## Access Control

- Use `Ownable2Step` over `Ownable` — prevents accidental ownership transfer to wrong address
- Prefer role-based access (`AccessControl`) over single-owner for multi-permission systems

## Upgradability

- If using proxies, follow storage gap pattern (`uint256[50] private __gap`)
- Never leave implementation contracts uninitialized — call `_disableInitializers()` in constructor

## Error Handling

- Revert early — place revert/validation checks at the top of functions
- Use descriptive error parameters (`Counter_InsufficientBalance(uint256 requested, uint256 available)`)

## Design Patterns

- Prefer composition over inheritance when possible
- Keep contracts focused — single responsibility
- Use interfaces for cross-contract communication
- Emit events for all state-changing operations
- Design for testability — avoid deep internal coupling that requires testing private functions
- Keep deployment logic in scripts, not in constructors
