---
name: review-contract
description: |
  Review a Solidity contract against project conventions for naming, NatSpec,
  gas patterns, security, and style. Use when checking code quality or reviewing
  contracts before merging.
user-invocable: true
disable-model-invocation: true
argument-hint: "[path/to/Contract.sol]"
---

# Contract Review

Review the Solidity contract at `$ARGUMENTS` against all project conventions.

## Setup

Before reviewing, read these rule files to load the project standards:

1. `.claude/rules/solidity-style.md` — naming, layout, NatSpec, pragma, imports
2. `.claude/rules/best-practices.md` — gas, security, access control, design
3. `.claude/rules/guardrails.md` — secrets, hardcoded values

## Review Checklist

### 1. Style Compliance
- Pragma version: `0.8.30` (exact) for src/scripts, `>=0.8.20 <0.9.0` for interfaces/libraries/tests
- Imports: absolute paths only (`import { X } from 'src/X.sol';`)
- Naming: contracts (PascalCase), interfaces (`I` prefix), libraries (`Lib` prefix),
  state vars (`s_` prefix), params (`_` prefix), returns (`_` suffix),
  errors (`Contract_ErrorName`), events (PascalCase with indexed `_` params)
- Section headers: `/* //////////////////////////////////////////////// */`
- NatSpec: `@title`, `@notice`, `@author` on contracts; `@notice`, `@param`, `@return` on functions

### 2. Contract Layout Order
Verify sections appear in this order:
1. Type declarations  2. State variables  3. Events  4. Errors
5. Modifiers  6. Constructor  7. Receive/fallback  8. External functions
9. Public functions  10. Internal functions  11. Private functions
(view/pure after state-changing within each group)

### 3. Gas Patterns
- Custom errors only (never `require("string")`)
- `++i` not `i++`
- Storage packing in structs
- `bytes32` over `string` for fixed-length
- Cache storage reads into local variables
- `immutable` for constructor-set values, `constant` for compile-time
- No magic numbers (except 0, 1, 1e18, type bounds)

### 4. Security
- Check-Effects-Interactions pattern
- ReentrancyGuard for untrusted external calls
- Input validation at system boundaries
- `address(0)` checks before external calls
- SafeERC20 for token interactions
- Pull over push for ETH transfers
- Ownable2Step over Ownable
- No hardcoded addresses, keys, or secrets

### 5. Design
- Single responsibility
- Composition over inheritance
- Events for all state changes
- Revert early (checks at top of functions)
- Descriptive error parameters

## Output Format

Organize findings by severity:

**Errors** (must fix before merge)
- [finding with file:line reference and the specific rule violated]

**Warnings** (should fix)
- [finding]

**Notes** (consider improving)
- [finding]

If the contract is clean, say so explicitly.
