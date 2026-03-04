# Guardrails

- Never hardcode addresses, private keys, or secrets in source files
- Never commit `.env` files — use `.env.example` as a value-less template

## Secrets Policy

Any file containing secrets (private keys, API keys, RPC URLs) must be gitignored. Secrets go in `.env` files only. Never hardcode secrets in Solidity, test files, or config files.
