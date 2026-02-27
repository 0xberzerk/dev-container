# CLAUDE.md

## Project Description

This is a **reusable Foundry-based Solidity development template**. It provides a fully configured starting point for smart contract projects with production-grade tooling, testing patterns, linting, CI/CD, and AI-assisted development support.

It is not a protocol or application — it is scaffolding. The `Counter` contract exists only as a reference example to demonstrate the established patterns.

## Tech Stack

- **Language:** Solidity `0.8.30`, EVM target `cancun`
- **Framework:** Foundry (forge, cast, anvil)
- **Optimizer:** enabled, 200 runs
- **Testing:** Foundry test (unit, integration via fork, invariant via handlers), BTT with bulloak
- **Linting:** solhint (tiered configs), forge fmt
- **CI/CD:** GitHub Actions (test, lint, coverage, Slither)
- **Git hooks:** Husky + lint-staged + commitlint

## Commands

```bash
forge build                                          # compile
forge test                                           # unit + invariant (5k fuzz runs)
forge test --match-path 'test/unit/**'               # unit only
forge test --match-path 'test/invariant/**'          # invariant only
FOUNDRY_PROFILE=integration forge test \
  --match-path 'test/integration/**'                 # integration (requires FORK_RPC_URL)
FOUNDRY_PROFILE=ci forge test                        # CI profile (10k fuzz runs)
forge fmt                                            # format
forge fmt --check                                    # check formatting
yarn lint                                            # solhint (src + test)
yarn lint:all                                        # fmt + solhint + bulloak check
bulloak check test/**/*.tree                         # verify tree ↔ test sync
bulloak scaffold -w test/**/path.tree                # generate test from tree
forge coverage --report lcov                         # coverage report
forge script script/Counter.s.sol --broadcast        # deploy (example)
```

## Project Structure

```
src/                          # Production contracts
  interfaces/                 # Interfaces (strict NatSpec, pragma >=0.8.20 <0.9.0)
  libraries/                  # Libraries (strict NatSpec, pragma >=0.8.20 <0.9.0)
test/
  Base.t.sol                  # Global test base (actors, funding)
  unit/
    <Contract>/
      <Contract>.t.sol        # Per-contract unit base (deploys directly)
      <function>/
        <function>.tree       # BTT spec (bulloak)
        <function>.t.sol      # Scaffolded + fuzzed tests
  integration/
    <Contract>/
      <Contract>.t.sol        # Per-contract integration base (fork + deploy script)
      <scenario>/
        <scenario>.tree
        <scenario>.t.sol
  invariant/
    handlers/
      <Contract>Handler.sol   # Handler (bounded actions, ghost vars)
    <Contract>Invariant.t.sol # Invariant assertions
script/                       # Deployment scripts
```

Conventions and rules are in `.claude/rules/`.
