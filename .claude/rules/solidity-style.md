# Solidity Style

## General

- **Pragma:** `0.8.30` (exact) for src contracts and scripts, `>=0.8.20 <0.9.0` for interfaces/libraries/tests
- **Imports:** absolute paths only (`import { Counter } from 'src/Counter.sol';`)
- **Section headers:** `/* //////////////////////////////////////////////// */` blocks
- **Formatting:** 2-space indent, 120-char line, single quotes, sorted imports (see `foundry.toml [fmt]`)

## Documentation
- **NatSpec:** block style `/** */` — required on all src contracts, functions, events, errors
  - Contracts: `@title`, `@notice`, `@author`, `@dev` (when needed)
  - Functions: `@notice`, `@param`, `@return`, `@dev` (when needed)

## Contract Layout

1. Type declarations (`using`, `struct`, `enum`)
2. State variables
3. Events
4. Errors
5. Modifiers
6. Constructor
7. Receive / fallback
8. External functions
9. Public functions
10. Internal functions
11. Private functions

Within each group, place `view`/`pure` functions after state-changing ones.

## Naming

| Element | Pattern | Example |
|---|---|---|
| Contract | PascalCase | `Counter`, `CounterHandler` |
| Interface | `I` prefix | `ICounter` |
| Library | `Lib` prefix | `LibCounter` |
| State variables | `s_` prefix | `s_counter` |
| Immutable variables | `i_` prefix | `i_counter` |
| Function | camelCase | `setNumber()`, `increment()` |
| Parameter | `_` prefix | `_newNumber`, `_amount` |
| Return value | `_` suffix | `result_`, `balance_` |
| Custom error | `Contract_ErrorName` | `Counter_NumberTooLarge` |
| Event | PascalCase, indexed params with `_` | `NumberSet(uint256 indexed _newNumber)` |
| Modifier (test) | `givenCondition` / `whenCondition` | `givenTheNumberIsZero` |
| Ghost variable | `ghost_` prefix | `ghost_number` |
| Call counter | `calls_` prefix | `calls_setNumber` |

## Variable Visibility (src/ contracts only)

- All state and immutable variables must be `internal` — never `public`
- Expose values via custom getter functions with the `get` prefix (e.g., `getNumber()`, `getCounterAddress()`)
- This rule does not apply to test files, scripts, or handlers

## Solhint Tiers

- `src/` — strict: compiler-version error, NatSpec warn, gas-custom-errors warn
- `src/interfaces/` — stricter: NatSpec error, interface-starts-with-i error
- `src/libraries/` — stricter: NatSpec error, gas-custom-errors error
- `test/` — permissive: NatSpec off, naming rules off, gas rules off
