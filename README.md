# Foundry Development Environment

A production-grade Foundry template for Solidity smart contract development.

This is **scaffolding, not a protocol**. The `Counter` contract exists only as a reference to demonstrate every pattern configured here. Click **"Use this template"** on GitHub to create your own repo, then delete Counter and start building.

## What's Included

| Layer | Tools | Why |
|-------|-------|-----|
| **Compiler** | Solidity 0.8.30, EVM target `cancun`, optimizer (200 runs) | Pinned versions prevent build drift across machines |
| **Testing** | BTT with bulloak, 3-tier tests (unit/integration/invariant), fuzz | Specification-first tests catch edge cases before deployment |
| **Linting** | solhint (tiered configs per directory), forge fmt | Consistent style eliminates PR noise |
| **CI/CD** | GitHub Actions (test, lint, coverage, Slither) | Automated quality gates on every push |
| **Git hooks** | Husky + lint-staged + commitlint | Catches issues locally before CI |
| **AI tooling** | Claude Code plugin (skills, agents, hooks) | Automates reviews, test generation, and enforces guardrails |

## Getting Started

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation) (forge, cast, anvil)
- [Node.js](https://nodejs.org/) >= 18 (for solhint, husky, commitlint)
- [bulloak](https://github.com/alexfertel/bulloak) (`cargo install bulloak`)

### Create Your Repo

1. Click **"Use this template"** > **"Create a new repository"** on GitHub
2. Clone your new repo and install dependencies:

```bash
git clone <your-repo-url> && cd <repo>
git submodule update --init --recursive   # forge-std
yarn install                               # solhint, husky, commitlint
```

3. Verify the setup:

```bash
forge build
forge test          # 7 tests (4 unit fuzzed, 3 invariant)
yarn lint:all       # fmt + solhint + bulloak check
```

### Clean Up the Example

Delete the `Counter` reference files and start with your own contracts:

- `src/Counter.sol`
- `src/interfaces/ICounter.sol`
- `test/unit/Counter/`
- `test/integration/Counter/`
- `test/invariant/CounterInvariant.t.sol`
- `test/invariant/handlers/CounterHandler.sol`
- `script/Counter.s.sol`

Keep `test/Base.t.sol` — it provides the shared actor setup for all test tiers.

## Project Structure

```
src/                              # Production contracts (pragma 0.8.30 exact)
  interfaces/                     # Interfaces (pragma >=0.8.20 <0.9.0)
  libraries/                      # Libraries (pragma >=0.8.20 <0.9.0)
test/
  Base.t.sol                      # Shared test base: actors, funding
  unit/<Contract>/
    <Contract>.t.sol              # Per-contract base (deploys directly)
    <function>/
      <function>.tree             # BTT spec (bulloak)
      <function>.t.sol            # Fuzzed tests scaffolded from tree
  integration/<Contract>/
    <Contract>.t.sol              # Per-contract base (fork + deploy script)
    <scenario>/
      <scenario>.tree
      <scenario>.t.sol
  invariant/
    handlers/<Contract>Handler.sol  # Bounded actions, ghost vars, call counters
    <Contract>Invariant.t.sol        # Invariant assertions
script/                           # Deployment scripts
.claude/
  rules/                          # AI convention rules (style, testing, guardrails)
  plugins/foundry-dev/            # Claude Code plugin (skills, agents, hooks)
```

**Why this structure?** Each test tier has its own deployment strategy and isolation level. BTT trees sit next to their tests so `bulloak check` can verify sync. Per-contract bases prevent setUp duplication.

## Commands

```bash
# Build
forge build

# Test
forge test                                         # unit + invariant (5k fuzz runs)
forge test --match-path 'test/unit/**'             # unit only
forge test --match-path 'test/invariant/**'        # invariant only
FOUNDRY_PROFILE=integration forge test \
  --match-path 'test/integration/**'               # integration (requires FORK_RPC_URL)
FOUNDRY_PROFILE=ci forge test                      # CI profile (10k fuzz runs)

# Lint & Format
forge fmt                                          # format
forge fmt --check                                  # check formatting
yarn lint                                          # solhint (src + test)
yarn lint:all                                      # fmt + solhint + bulloak check

# BTT
bulloak check test/**/*.tree                       # verify tree <> test sync
bulloak scaffold -w test/**/path.tree              # generate test from tree

# Other
forge coverage --report lcov                       # coverage report
forge script script/Counter.s.sol --broadcast      # deploy (example)
```

## Testing

### Three Tiers

| Tier | Deploys via | Runs | External deps | Purpose |
|------|-------------|------|---------------|---------|
| **Unit** | `new Contract()` | 5,000 fuzz | mocked (`vm.mockCall`) | Isolated function logic |
| **Integration** | production deploy script | 1,000 fuzz | real (forked chain) | End-to-end on mainnet state |
| **Invariant** | handler wrapping target | 256 runs × 100 depth | direct | Protocol-wide properties hold under random sequences |

**Why three tiers?** Unit tests are fast and isolated but miss deployment issues. Integration tests catch real-world interactions but are slow and flaky. Invariant tests find state-dependent bugs that neither tier covers alone.

> **Invariant tuning:** The default `runs = 256, depth = 100` is kept moderate for fast local iteration. CI bumps to `512 × 200`. For real protocols, consider increasing to `runs = 1000+, depth = 500+` in `foundry.toml`.

> **Note:** Integration tests require a `FORK_RPC_URL` environment variable pointing to an RPC endpoint (e.g. Alchemy, Infura). When unset, the fork is not created but the tests still execute against a blank local chain — they will not automatically skip. Set the variable in a `.env` file or pass it inline.

### BTT (Branching Tree Technique)

Every function gets a `.tree` file defining all execution branches **before** writing tests:

```
SetNumberUnitTest
├── when the new number exceeds uint128 max
│   └── it should revert with Counter_NumberTooLarge.
└── when the new number is within bounds
    ├── it should store the number.
    └── it should emit a NumberSet event.
```

`bulloak scaffold -w` generates the test skeleton. Fuzz parameters and assertions are added manually.

**Why BTT?** Writing the spec first forces you to think about every branch. `bulloak check` ensures tests stay in sync with the spec — if you add a revert path to a function, the tree catches the missing test.

### Fuzz Testing

All unit tests accept fuzz parameters. Inputs are constrained with `bound()`, never `vm.assume()`.

**Why `bound()` over `vm.assume()`?** `vm.assume()` discards runs that don't match, wasting fuzzer iterations. `bound()` maps every input to the valid range — no wasted runs.

## Linting

Solhint uses **tiered configs** — each directory has rules matching its purpose:

| Directory | Strictness | Key rules |
|-----------|------------|-----------|
| `src/` | strict | compiler-version error, NatSpec warn, gas-custom-errors warn |
| `src/interfaces/` | stricter | NatSpec error, interface-starts-with-i error |
| `src/libraries/` | stricter | NatSpec error, gas-custom-errors error |
| `test/` | permissive | NatSpec off, naming rules off, gas rules off |

**Why tiered?** Production contracts need strict documentation and gas checks. Test files use unconventional naming (`test_WhenX`, `ghost_y`) that would trigger false positives under strict rules.

## Pragma Policy

| Scope | Pragma | Rationale |
|-------|--------|-----------|
| `src/`, `script/` | `0.8.30` (exact) | Deployed contracts must compile with a known, audited compiler version |
| `src/interfaces/`, `src/libraries/` | `>=0.8.20 <0.9.0` | Reusable across projects with different compiler versions |
| `test/` | `>=0.8.20 <0.9.0` | Flexible — tests are never deployed |

## CI/CD

Four GitHub Actions workflows run on every push and PR:

| Workflow | What it does |
|----------|-------------|
| **test.yml** | `forge build --sizes` + `forge test` (CI profile, 10k fuzz runs) |
| **lint.yml** | `forge fmt --check` + `solhint` + `bulloak check` |
| **coverage.yml** | `forge coverage --report lcov`, enforces 90% minimum line coverage |
| **slither.yml** | Slither static analysis, fails on high-severity findings |

All workflows use concurrency groups to cancel stale runs.

### Recommended Branch Protection

In your GitHub repo settings under **Branches > Branch protection rules** for `main`:

- **Require status checks to pass:** `Build & Test`, `Formatting`, `Solhint`, `Bulloak Check`, `Code Coverage`, `Static Analysis`
- **Require branches to be up to date before merging**
- **Require pull request reviews before merging** (at least 1 approval)

## Git Conventions

### Commits

Format: `type(scope): description` — enforced by commitlint via `commit-msg` hook.

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`, `setup`, `hotfix`

### Branches

Format: `type/kebab-case-description` — enforced by `pre-push` hook.

### Hooks

| Hook | Runs |
|------|------|
| **pre-commit** | `lint-staged` (forge fmt + solhint on staged .sol files) |
| **commit-msg** | commitlint validation |
| **pre-push** | branch name check + `forge test` |

## AI Tooling (Claude Code)

The `.claude/` directory contains project conventions and a development plugin:

- **`.claude/rules/`** — Style, testing, security, and git conventions loaded automatically by Claude Code
- **`.claude/plugins/foundry-dev/`** — Plugin with skills, agents, and hooks

### Skills

| Skill | Usage | Purpose |
|-------|-------|---------|
| `commit` | `/foundry-dev:commit` | Smart conventional commit from staged changes |
| `btt-workflow` | `/foundry-dev:btt-workflow Counter setNumber` | Guided BTT workflow: tree, scaffold, implement, verify |
| `review-contract` | `/foundry-dev:review-contract src/Counter.sol` | Review contract against all project conventions |
| `pre-deploy` | `/foundry-dev:pre-deploy` | Full pre-deployment verification checklist |

### Agents

| Agent | Purpose |
|-------|---------|
| **solidity-reviewer** | Read-only contract reviewer (auto-triggered on code review tasks) |
| **test-writer** | Generates BTT tests from contract changes |

### Hooks

| Event | Behavior |
|-------|----------|
| **PreToolUse** (Bash) | Blocks `--no-verify`, destructive `rm` on project dirs, force push to main |
| **PostToolUse** (Write/Edit) | Reminds to run `forge fmt` after modifying .sol files |
| **Stop** | Reminds to run `bulloak check` if .tree/.t.sol files were touched |

## Naming Conventions

| Element | Pattern | Example |
|---------|---------|---------|
| Contract | PascalCase | `Counter` |
| Interface | `I` prefix | `ICounter` |
| Library | `Lib` prefix | `LibCounter` |
| State variable | `s_` prefix | `s_counter` |
| Function | camelCase | `setNumber()` |
| Parameter | `_` prefix | `_newNumber` |
| Return value | `_` suffix | `result_` |
| Custom error | `Contract_ErrorName` | `Counter_NumberTooLarge` |
| Event | PascalCase | `NumberSet(uint256 indexed _newNumber)` |

**Why prefixes/suffixes?** They eliminate ambiguity between state variables, parameters, and return values at a glance — no need to scroll to declarations.

## License

[MIT](LICENSE)
