---
name: poc
description: "Generate a targeted exploit test (POC) for a confirmed security finding. Creates a Foundry test in test/poc/ that proves the vulnerability is exploitable."
user-invokable: true
disable-model-invocation: true
argument-hint: "<finding description or @audit-confirmed reference>"
---

# POC Writer

Generate a targeted exploit test that proves a specific vulnerability is exploitable.

## Context

You are writing a POC for a security audit. The auditor has confirmed a finding and wants a concrete exploit test that demonstrates the impact. This is NOT a unit test — it's a minimal proof that the bug is real and exploitable.

## Inputs

The auditor provides one of:
- A finding description (vulnerability, location, exploit idea)
- A reference to an `@audit-confirmed` tag in the source code
- A finding ID from `analysis/consolidated-findings.json` or `analysis/final-report.md`

## Step 1 — Understand the Finding

1. If given a finding ID or reference, read the relevant pipeline output:
   - `analysis/consolidated-findings.json`
   - `analysis/intersection-analysis.json`
   - `analysis/final-report.md`
2. Read the vulnerable source code at the referenced location
3. Identify: what breaks, how an attacker triggers it, what the impact is

## Step 2 — Read the Test Base

Read `test/Base.t.sol` to understand:
- Available actors: `attacker`, `victim`, `admin`, `protocolOwner`
- Setup helpers, fork configuration, label conventions
- Any shared state or deployment logic

## Step 3 — Design the Exploit

Before writing code, outline:
1. **Setup** — what contract state is needed for the exploit?
2. **Pre-conditions** — what assertions prove the starting state is honest?
3. **Attack** — what exact steps does the attacker take? (be specific: which functions, which parameters, in what order)
4. **Post-conditions** — what assertions prove the exploit worked? (attacker gained value, victim lost value, invariant broke)

Present this outline to the auditor for confirmation before writing the test.

## Step 4 — Write the POC

Create a new test file in `test/poc/`. Naming: `POC_<ShortDescription>.t.sol`

### Structure

```solidity
// SPDX-License-Identifier: UNLICENSED
pragma solidity <target version>;

import {Base} from "../Base.t.sol";

/// @title POC: <short description>
/// @notice Proves that <vulnerability summary>
contract POC_ShortDescription is Base {
    // --- Setup specific to this exploit ---

    function setUp() public override {
        super.setUp();
        // Deploy or configure contracts into the vulnerable state
        // Label addresses for readable traces
    }

    function test_poc_description() public {
        // --- Pre-conditions ---
        // Assert the honest starting state

        // --- Attack ---
        vm.startPrank(attacker);
        // Execute exploit steps
        vm.stopPrank();

        // --- Post-conditions ---
        // Assert the exploit outcome:
        //   - Attacker profit
        //   - Victim loss
        //   - Invariant violation
        //   - Unauthorized state change
    }
}
```

### Rules

- **One test per POC file.** Each POC proves one vulnerability. Multiple exploit paths for the same bug get separate test functions in the same file.
- **Fork-first.** If the exploit depends on on-chain state, use `vm.createSelectFork(vm.envString("FORK_RPC_URL"), blockNumber)` in setUp. Pin to a specific block.
- **Label everything.** Use `vm.label(address, "name")` for every address that appears in traces.
- **Minimize setup.** Only deploy what the exploit needs. Don't reproduce the entire protocol if you only need two contracts.
- **Concrete values.** Use specific amounts that demonstrate the impact (e.g., "attacker profits 1000 USDC" not "attacker profits some amount"). Calculate expected values explicitly.
- **No mocks for the vulnerable code.** Mock external dependencies if needed, but never mock the contract being exploited.
- **Comments explain the attack, not the Solidity.** Comments should narrate the exploit flow, not explain what `vm.prank` does.

## Step 5 — Run and Verify

Run the POC:

```bash
forge test --match-path 'test/poc/POC_*.t.sol' -vvvv
```

- If it passes: the exploit is confirmed. Show the auditor the trace highlights (value transfers, state changes).
- If it fails: diagnose why. Is the setup wrong? Is there a protection the auditor missed? Report back — a failing POC is also valuable information.

## Output

Present to the auditor:
1. The POC file path
2. A brief summary: what the POC proves, the attack flow, and the concrete impact (numbers)
3. The test result (pass/fail) with relevant trace output
