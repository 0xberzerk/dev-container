---
name: pre-deploy
description: |
  Run pre-deployment verification checklist. Checks tests, formatting, linting,
  and security before deploying contracts. Use before any deployment to mainnet
  or testnet.
user-invocable: true
disable-model-invocation: true
---

# Pre-Deploy Checklist

Run a full verification before deploying contracts.

## Setup

Read `.claude/rules/guardrails.md` for the secrets and key management policies.

## Checks

Run each check sequentially and report pass/fail:

### 1. Compilation
```bash
forge build
```
Must complete with zero errors.

### 2. Tests
```bash
forge test
```
All unit and invariant tests must pass.

### 3. Formatting
```bash
forge fmt --check
```
Must report no formatting differences.

### 4. Linting
```bash
yarn lint
```
Must complete with zero errors (warnings acceptable).

### 5. BTT Sync
```bash
bulloak check test/**/*.tree
```
All `.tree` files must be in sync with their `.t.sol` counterparts.

### 6. Gas Report
```bash
forge test --gas-report
```
Review gas consumption for deployment and function calls. No automated threshold,
but flag any function exceeding 100k gas for the user's attention.

### 7. Git State
- Verify current branch follows `type/kebab-case` naming
- Verify working tree is clean (`git status`)
- Verify no `.env` files are staged: `git diff --cached --name-only`

### 8. Secrets Check
- No hardcoded addresses in `src/` (except well-known protocol addresses)
- No private keys or API keys in any tracked file
- Deployment should use Foundry encrypted keystores, not plaintext keys

## Output

Summarize results in a table:

| Check | Status | Notes |
|-------|--------|-------|
| Compilation | ... | ... |
| Tests | ... | ... |
| Formatting | ... | ... |
| Linting | ... | ... |
| BTT sync | ... | ... |
| Gas report | ... | ... |
| Git state | ... | ... |
| Secrets | ... | ... |

If all checks pass, confirm the project is ready for deployment.
If any check fails, list what needs to be fixed before proceeding.
