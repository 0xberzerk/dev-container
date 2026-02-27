# Testing Patterns

## BTT (Branching Tree Technique) + bulloak

1. Write `.tree` file defining all branches (`given`/`when`/`it`)
2. Run `bulloak scaffold -w` to generate test skeleton
3. Add fuzz parameters and assertions manually
4. Keep tree ↔ test in sync (`bulloak check`)

### bulloak Constraints

- **`given`** (modifiers): reuse identical names across branches — avoids duplicated logic
- **`when`** (function-level leaves): must be unique — bulloak reverts on duplicate function names
- **`it`** (descriptions): can be duplicated — these are comments only, not generated code

## Keywords

- **`given`** — state precondition: generates modifier that sets contract state
- **`when`** — input condition: bounding done in function body, no state modifier
- **`it`** — expected outcome (leaf node in tree)

## Three-tier Tests

| Tier | Deploys via | Fuzz runs | External deps |
|---|---|---|---|
| Unit | direct `new Contract()` | 5,000 | mocked (`vm.mockCall`) |
| Integration | production deploy script | 1,000 | real (forked chain) |
| Invariant | handler wrapping target | 5,000 | direct |

## Inheritance

```
Test (forge-std)
  └── BaseTest (actors, funding)
        ├── UnitCounterTest (deploys Counter)
        │     ├── setNumber tests
        │     └── increment tests
        ├── IntegrationCounterTest (fork + script)
        │     └── setThenIncrement tests
        └── Counter invariant test (handler + targetContract)
```

## Fuzz Testing Rules

- Use `bound(input, min, max)` to constrain fuzz inputs, not `vm.assume()`
- All unit test functions should accept fuzz parameters where applicable
- Integration tests: fork at a pinned block for determinism

## Invariant Testing Rules

- Handler contracts wrap target functions with bounded inputs
- Unbounded target functions should still be called to ensure all state is covered
- Track expected state with `ghost_` variables
- Track call distribution with `calls_` counters
- Use `targetContract()` to restrict fuzzer to non pure functions

## Workflows

### Function Changed (added/removed checks, signature change, new revert paths)

1. Update the `.tree` file to reflect new/removed/changed branches
2. Run `bulloak scaffold -w` to generate new stubs (preserves existing ones)
3. Update affected test logic (assertions, fuzz bounds, modifiers)
4. If the function is wrapped by an invariant handler, update the handler accordingly
5. Verify: `bulloak check`, `forge test`
