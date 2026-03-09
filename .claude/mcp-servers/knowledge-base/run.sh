#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Source .env if it exists (for any future KB-specific env vars)
if [[ -f "$PROJECT_ROOT/.env" ]]; then
  set -a
  source "$PROJECT_ROOT/.env"
  set +a
fi

# Default KB directory to project root's KnowledgeBase/
export KB_DIR="${KB_DIR:-$PROJECT_ROOT/KnowledgeBase}"

exec "$SCRIPT_DIR/target/release/knowledge-base"
