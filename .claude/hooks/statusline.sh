#!/usr/bin/env bash
# statusline.sh — renders a persistent status bar in Claude Code
# Shows: git branch | context tokens | session cost
set -euo pipefail

SEGMENTS=()

# --- Git branch ---
BRANCH=$(git branch --show-current 2>/dev/null || echo "")
if [ -n "$BRANCH" ]; then
  SEGMENTS+=("$BRANCH")
fi

# --- Context tokens (best-effort) ---
if [ -n "${CLAUDE_CONTEXT_TOKENS_USED:-}" ] && [ -n "${CLAUDE_CONTEXT_WINDOW:-}" ]; then
  USED_K=$((CLAUDE_CONTEXT_TOKENS_USED / 1000))
  WINDOW_K=$((CLAUDE_CONTEXT_WINDOW / 1000))
  SEGMENTS+=("ctx: ${USED_K}k/${WINDOW_K}k")
fi

# --- Session cost (best-effort) ---
if [ -n "${CLAUDE_SESSION_COST:-}" ]; then
  SEGMENTS+=("\$${CLAUDE_SESSION_COST}")
fi

# --- Render ---
if [ ${#SEGMENTS[@]} -gt 0 ]; then
  IFS=" | "
  echo "${SEGMENTS[*]}"
fi
