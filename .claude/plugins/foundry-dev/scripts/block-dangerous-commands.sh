#!/bin/bash
# PreToolUse hook: block dangerous Bash commands per project guardrails
# See .claude/rules/guardrails.md and .claude/rules/git-conventions.md

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# Block destructive rm on critical directories
if echo "$COMMAND" | grep -qE 'rm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+|.*-rf\s+)(src|test|script|lib)/'; then
  echo "Blocked: destructive deletion of project directories is not allowed" >&2
  exit 2
fi

# Block --no-verify (guardrails.md)
if echo "$COMMAND" | grep -q '\-\-no-verify'; then
  echo "Blocked: --no-verify is prohibited by project guardrails" >&2
  exit 2
fi

# Block force push to main/develop
if echo "$COMMAND" | grep -qE 'git\s+push\s+.*--force.*\b(main|develop)\b'; then
  echo "Blocked: force push to main/develop is not allowed" >&2
  exit 2
fi
if echo "$COMMAND" | grep -qE 'git\s+push\s+.*\b(main|develop)\b.*--force'; then
  echo "Blocked: force push to main/develop is not allowed" >&2
  exit 2
fi

exit 0
