---
name: btt-workflow
description: |
  Guide through the BTT (Branching Tree Technique) testing workflow. Creates
  .tree files, scaffolds tests with bulloak, and implements test logic following
  project patterns. Use when adding or modifying contract functions that need tests.
user-invocable: true
disable-model-invocation: true
argument-hint: "[ContractName] [functionName]"
---

# BTT Workflow

Generate BTT-structured tests for `$0.$1()` following the project's testing patterns.

## Setup

1. Read `.claude/rules/testing.md` for the complete BTT methodology and rules.
2. Read the target function in `src/$0.sol` to understand its behavior, branches,
   and revert conditions.

## Reference Examples

Study these existing files to match the established patterns:

- **Tree files:** `test/unit/Counter/setNumber/setNumber.tree`,
  `test/unit/Counter/increment/increment.tree`
- **Test files:** `test/unit/Counter/setNumber/setNumber.t.sol`,
  `test/unit/Counter/increment/increment.t.sol`
- **Unit base:** `test/unit/Counter/Counter.t.sol`
- **Global base:** `test/Base.t.sol`

## Step-by-Step

### 1. Analyze the function

Read `src/$0.sol` and identify:
- All revert paths (each becomes a `when` branch)
- State preconditions (each becomes a `given` branch with a modifier)
- Success outcomes (each becomes an `it` leaf)
- Events emitted (each gets an `it should emit` leaf)
- State changes (each gets an `it should store/update` leaf)

### 2. Create the `.tree` file

Write to `test/unit/$0/$1/$1.tree`.

Tree format — the root node is the test contract name (`<Function>UnitTest`):

```
SetNumberUnitTest
├── when the new number exceeds uint128 max
│   └── it should revert with Counter_NumberTooLarge.
└── when the new number is within bounds
    ├── it should store the number.
    └── it should emit a NumberSet event.
```

**BTT keyword rules:**
- **`given`** = state precondition. Generates a modifier that sets contract state.
  Reuse identical `given` names across branches (bulloak deduplicates modifiers).
- **`when`** = input condition. Bounding done in the function body, not a modifier.
  Must be unique across branches (bulloak reverts on duplicate function names).
- **`it`** = expected outcome (leaf). Can be duplicated (comments only, not code).

### 3. Scaffold with bulloak

```bash
bulloak scaffold -w test/unit/$0/$1/$1.tree
```

This generates the test skeleton. If the test file already exists, bulloak merges
new branches without overwriting existing implementations.

### 4. Implement test logic

After scaffolding, fill in each test function:

- **Add fuzz parameters** to every test function where applicable.
  bulloak does not generate fuzz params — add them manually.
- **Use `bound()`** to constrain fuzz inputs. Never use `vm.assume()`.
- **Add assertions:** `assertEq`, `vm.expectRevert`, `vm.expectEmit`.
- **Implement `given` modifiers** to set up state preconditions.
- Ensure the test contract inherits from the correct unit base
  (e.g., `CounterUnitTest` which deploys `Counter` in `setUp()`).

### 5. Verify

Run these commands sequentially:

```bash
bulloak check test/**/*.tree
forge test --match-path 'test/unit/$0/$1/*'
```

### 6. Check invariant impact

If the function is wrapped by a handler in `test/invariant/handlers/`, check
whether the handler needs updating (new bounds, new ghost variables, new
call counters).

## Rules

- All unit test functions should accept fuzz parameters where applicable
- Use `bound(input, min, max)` to constrain fuzz inputs, not `vm.assume()`
- Fuzz runs: 5,000 on default profile, 10,000 on CI
- Import style: absolute paths only (`import { X } from 'src/X.sol';`)
- NatSpec: `@title`, `@notice`, `@author` on the test contract
