# Security Review Focus Areas

## What to Look For

The goal is not to check if code follows conventions. It's to find the small thing that, under a specific condition, triggers a massive loss.

### State Machine Inconsistencies
- Can the contract reach a state the developer didn't anticipate?
- Are there unreachable states that should be reachable, or vice versa?
- Can state transitions be forced out of order?

### Trust Boundary Violations
- Where does the code assume trusted input from untrusted sources?
- Are there privilege escalation paths through chained calls?
- Can an unprivileged user trigger a privileged code path indirectly?

### Edge Condition Interactions
- What happens when two features interact at boundary values?
- Are there off-by-one errors at min/max boundaries?
- What about empty arrays, zero amounts, max uint values?

### Reentrancy Paths
- Not just the obvious single-function reentrancy — look for cross-function and cross-contract paths
- Read-only reentrancy (view functions returning stale state during external calls)
- State corruption through callback ordering

### Oracle and Price Manipulation
- Stale data windows (how old can oracle data be?)
- Sandwich opportunities around price-sensitive operations
- TWAP manipulation feasibility (liquidity depth vs. manipulation cost)
- Fallback oracle behavior — what happens when the primary fails?

### Rounding and Precision Loss
- Truncation that only matters at specific boundaries (first deposit, last withdrawal)
- Division before multiplication chains
- Accumulator drift over many operations
- Share price manipulation in vault-like contracts

### Access Control Gaps
- Missing checks on state-changing functions
- Unprotected initializers (especially in proxy patterns)
- Privilege escalation through role manipulation
- Time-locked operations that can be bypassed

### Token Assumption Failures
- Fee-on-transfer tokens breaking balance accounting
- Rebasing tokens corrupting share calculations
- Tokens with blocklists (USDC, USDT) causing DoS
- Missing return value checks (non-standard ERC20)
- Tokens with different decimals than expected

### Temporal Dependencies
- `block.timestamp` manipulation windows (up to ~12s on Ethereum)
- Deadline bypasses through frontrunning
- Ordering assumptions that MEV can violate

### Economic Invariant Violations
- Can an attacker extract more value than they deposit?
- Flash loan attack vectors (borrow → manipulate → profit → repay in one tx)
- Donation attacks on share-based systems
- Griefing attacks (making operations unprofitable for others)
