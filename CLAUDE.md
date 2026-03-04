# CLAUDE.md

## Project Description

This is a **secure, containerized environment for auditing untrusted Solidity code**. It provides sandboxed, reproducible tooling for security auditors who review smart contracts, write exploit POCs, and run analysis tools.

The agents and tools act as an **extension of the auditor's brain** — optimizing the process, making audits faster and more accurate. The auditor works manually in parallel; agent output is documentation that augments their own findings.

## Tech Stack

- **Language:** Solidity (target protocol version)
- **Framework:** Foundry (forge, cast, anvil, chisel)
- **Static Analysis:** Slither, Aderyn
- **Fuzzing:** Foundry fuzz, Chimera (Recon — Echidna, Medusa, Halmos)
- **Compiler:** `solc 0.8.30`, EVM target `cancun`, optimizer 200 runs

## Commands

```bash
forge build                                          # compile
forge test                                           # all tests (poc + invariant + fuzz)
forge test --match-path 'test/poc/**'                # POC tests only
forge test --match-path 'test/invariant/**'          # invariant tests only
forge test --match-path 'test/fuzz/**'               # fuzz tests only
FOUNDRY_PROFILE=ci forge test                        # CI profile (10k fuzz runs)
forge fmt                                            # format
forge coverage --report lcov                         # coverage report
```

## Project Structure

```
src/                          # Target protocol contracts (cloned or symlinked)
  interfaces/                 # Protocol interfaces
  libraries/                  # Protocol libraries
test/
  Base.t.sol                  # Global test base (audit actors: attacker, victim, admin, protocolOwner)
  poc/                        # Targeted exploit tests for confirmed findings
  invariant/                  # Protocol invariant tests
    handlers/                 # Handler contracts wrapping targets
  fuzz/                       # Fuzz test suites
analysis/                     # Static analysis output (gitignored)
  slither.json                # Slither raw output
  aderyn.json                 # Aderyn raw output
  static-analysis.json        # Normalized merged output
KnowledgeBase/                # Curated vulnerability index (gitignored)
  raw/                        # Layer 1 — raw API cache
  curated/                    # Layer 2 — ranked by severity
  seeds/                      # Auditor pre-seeded entries
```

## Audit Workflow

1. **Annotate:** Auditor reads codebase, annotates with `@audit` tags
2. **Static Analysis:** Slither + Aderyn run as pre-processing
3. **Architect P1:** Parses tags + static output, builds codebase profile, launches specialists
4. **Specialists + Fuzz:** Run in parallel — domain experts + coverage engineer
5. **Consolidator:** Deduplicates, normalizes, routes fuzz results to specialists
6. **Architect P2:** Cross-domain interaction analysis
7. **Auditor Review:** Reviews findings, responds with post-pipeline `@audit` tags, invokes `/poc`

## @audit Tag Taxonomy

**Pre-pipeline** (auditor annotates before launching agents):
- `@audit-integration` — external protocol or standard used
- `@audit-trust-boundary` — who is trusted vs untrusted
- `@audit-attention` — specific concern, smell, or suspicious pattern
- `@audit-entry` — key entry point / attack surface

**Post-pipeline** (auditor reviews findings):
- `@audit-confirmed` — agrees with finding
- `@audit-false-positive` — rejects finding with reason
- `@audit-discuss` — wants further analysis
- `@audit-escalate` — upgrades severity

## Key Principles

- **No style enforcement on target code** — third-party code won't follow our conventions
- **Fork-first testing** — auditors work against forked chains by default
- **Severity scope:** Critical, high, and medium only — low/info/gas is noise
- **Human-first:** Agents amplify the auditor, they don't replace them
- **POCs are human-invoked** — `/poc` skill, not automated

Conventions and rules are in `.claude/rules/`.
