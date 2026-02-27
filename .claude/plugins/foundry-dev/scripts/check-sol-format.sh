#!/bin/bash
# PostToolUse hook: remind to run forge fmt after modifying .sol files

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

if [[ "$FILE_PATH" == *.sol ]]; then
  echo "A .sol file was modified. Run \`forge fmt\` to ensure formatting compliance before committing." >&2
fi

exit 0
