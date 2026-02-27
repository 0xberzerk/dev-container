# Guardrails

- Never hardcode addresses, private keys, or secrets in source files
- Never commit `.env` files — use `.env.example` as a value-less template
- Never bypass git hooks with `--no-verify`

## Secrets Policy

Any file containing secrets (private keys, API keys, RPC URLs) must be gitignored. Secrets go in `.env` files only. Never hardcode secrets in Solidity, scripts, or config files.

## Key Management

- Use Foundry encrypted keystores (`cast wallet import`) for deployment — never store private keys in `.env` files
- CI/CD must use encrypted keys or hardware-backed signers, never plaintext private keys
