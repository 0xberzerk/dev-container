---
name: review-contract
description: "Security-focused review of a Solidity contract. Analyzes for vulnerabilities, state machine inconsistencies, trust boundary violations, and exploit scenarios. Not a style checker — focused on finding what breaks."
user-invokable: true
disable-model-invocation: true
argument-hint: "[path/to/Contract.sol]"
---

# Security Review

Review the Solidity contract at `$ARGUMENTS` for security vulnerabilities.

## Setup

Before reviewing, read the project rules:

1. `.claude/rules/best-practices.md` — security review focus areas
2. `.claude/rules/guardrails.md` — secrets, hardcoded values

## Review Checklist

### 1. State Machine Analysis
- Map all state transitions — can any be forced out of order?
- Are there unreachable states that should be reachable?
- Can state be corrupted through unexpected call sequences?

### 2. Trust Boundaries
- Who can call each external/public function?
- Are there privilege escalation paths?
- Where does the code assume trusted input from untrusted sources?
- Are there unprotected initializers?

### 3. Value Flow
- Track all value movements (ETH, tokens, shares)
- Can an attacker extract more value than they should?
- Are there rounding issues at boundaries (first deposit, last withdrawal)?
- Flash loan attack vectors?

### 4. External Interactions
- Check-Effects-Interactions pattern compliance
- Reentrancy paths (single, cross-function, cross-contract, read-only)
- Unchecked return values on external calls
- Token assumption failures (fee-on-transfer, rebasing, blocklists)

### 5. Access Control
- Missing checks on state-changing functions
- Role manipulation paths
- Time-lock bypasses

### 6. Edge Conditions
- Zero amounts, empty arrays, max uint values
- Division by zero paths
- Overflow at boundary values (even with Solidity 0.8+ checked math)

## Output Format

Organize findings by severity:

**Critical** (direct fund loss or protocol compromise)
- [finding with file:line, exploit scenario, and impact]

**High** (conditional fund loss or significant protocol disruption)
- [finding]

**Medium** (unexpected behavior, griefing, or value leakage under specific conditions)
- [finding]

If the contract is clean, say so — but explain what you checked and why you're confident.
