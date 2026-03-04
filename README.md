# Secure Dev Container for Solidity Auditing

A containerized environment for auditing untrusted Solidity code with AI-assisted analysis.

## What This Is

A sandboxed, reproducible workspace for security auditors. It provides:

- **Foundry** for compilation, testing, and fork-based POC writing
- **Slither + Aderyn** for static analysis
- **Chimera/Recon** for advanced fuzzing (Echidna, Medusa, Halmos)
- **AI agent pipeline** that augments manual audit findings
- **Network isolation** to safely analyze untrusted code

## Quick Start

```bash
# 1. Clone this repo and open in VS Code with Dev Containers extension
# 2. Set up your environment
cp .env.example .env
# Edit .env with your RPC URL

# 3. Clone or symlink the target protocol into src/
# 4. Build
forge build

# 5. Annotate the codebase with @audit tags
# 6. Run the pipeline
```

## Project Structure

```
src/                    # Target protocol contracts
test/
  Base.t.sol            # Audit actors (attacker, victim, admin, protocolOwner)
  poc/                  # Exploit POC tests
  invariant/            # Invariant tests
  fuzz/                 # Fuzz test suites
analysis/               # Static analysis output (gitignored)
KnowledgeBase/          # Curated vulnerability index (gitignored)
```

## Documentation

- `CLAUDE.md` — Full workflow, tag taxonomy, and project conventions
- `.claude/TODO.md` — Architecture design and implementation roadmap
- `.claude/rules/` — Review focus areas, testing patterns, guardrails

## License

MIT
