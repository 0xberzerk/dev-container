# Git Conventions

## Commits

Format: `type(scope): description` (scope optional)

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`, `setup`, `hotfix`

Enforced by commitlint via `commit-msg` hook.

## Branches

Format: `type/kebab-case-description`

Exceptions: `main`, `develop`

Enforced by pre-push hook.

## Hooks (husky)

- **pre-commit:** `lint-staged` (forge fmt + solhint on staged .sol files)
- **commit-msg:** commitlint validation
- **pre-push:** branch name check + `forge test`
