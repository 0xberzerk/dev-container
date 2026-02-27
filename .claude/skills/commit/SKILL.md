---
name: commit
description: |
  Create a conventional commit following project conventions. Analyzes staged
  changes and generates type(scope): description format. Use when committing
  changes to the repository.
user-invocable: true
disable-model-invocation: true
---

# Commit Workflow

Create a git commit following this project's conventional commit format.

## Steps

1. **Read the rules.** Load `.claude/rules/git-conventions.md` for the commit format and
   `commitlint.config.js` for the canonical list of allowed types.

2. **Inspect staged changes.** Run these commands in parallel:
   - `git status` to see all staged and untracked files
   - `git diff --cached` to see the full staged diff
   - `git log --oneline -10` to see recent commit style

3. **Analyze the diff.** Determine the correct commit type from the allowed list:
   `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`, `setup`

   Choose based on what the changes actually do:
   - `feat` = wholly new feature or capability
   - `fix` = bug fix
   - `refactor` = code restructuring without behavior change
   - `test` = adding or updating tests
   - `docs` = documentation only
   - `style` = formatting, whitespace, semicolons (no logic change)
   - `build` = build system or dependency changes
   - `ci` = CI/CD configuration
   - `chore` = maintenance tasks
   - `setup` = project scaffolding and configuration

4. **Determine scope** (optional). If changes are isolated to a single contract,
   module, or subsystem, use it as scope: `feat(counter): add max value guard`.

5. **Draft the commit message.** Write 1-2 concise sentences focusing on **why**,
   not what. The diff already shows what changed.

6. **Check for secrets.** Verify no `.env` files, private keys, API keys, or RPC URLs
   are staged. If found, warn the user and do NOT commit.

7. **Present the message** to the user for approval before committing.

8. **Commit** using a HEREDOC for formatting:
   ```bash
   git commit -m "$(cat <<'EOF'
   type(scope): description

   Co-Authored-By: Claude <noreply@anthropic.com>
   EOF
   )"
   ```

## Rules

- Never use `--no-verify` (project guardrail)
- Never amend a previous commit unless explicitly asked
- If pre-commit hook fails, fix the issue and create a NEW commit
- Stage specific files by name, never use `git add -A` or `git add .`
