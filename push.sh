#!/usr/bin/env bash
# iron-sim push script — run this from your machine's terminal or as root.
# Usage:
#   GIT_PAT=<your-token> ./push.sh [OWNER/REPO]
# OWNER/REPO defaults to iron-sim/iron-sim if not given.
# The token is used only to create the remote repo and to push once.
#
# Requirements:
#   - bash
#   - git
#   - a Personal Access Token from github.com/settings/tokens (classic, scope: repo)
#
# After pushing, the CI/dev machine can:
#   git clone https://github.com/OWNER/iron-sim.git
#   cd iron-sim && source ~/.cargo/env && cd engine && cargo test --workspace
#
# For the future MCP server, use: cargo install iron-sim --path engine/crates/mcp  (crate to be created in Fase 3)
set -euo pipefail

TOKEN="${GIT_PAT:-}"
DEST="${1:-iron-sim/iron-sim}"
MONOREPO="${IRON_SIM_DIR:-/root/Projects/iron-sim}"
REMOTE="https://github.com/${DEST}.git"

if [ -z "$TOKEN" ]; then
  echo "Set GIT_PAT=<github token> before running." >&2
  exit 1
fi

# Create the repo via GitHub API if it does not exist
if ! curl -fsSL -H "Authorization: Bearer $TOKEN" "https://api.github.com/repos/$DEST" >/dev/null 2>&1; then
  echo "Creating ${DEST} via API..."
  curl -fsSL -X POST -H "Authorization: Bearer $TOKEN" \
    -d '{"name":"iron-sim","private":false,"auto_init":false,"has_issues":true,"has_wiki":true}' \
    "https://api.github.com/user/repos" >/dev/null
  echo "Created."
else
  echo "$DEST already exists."
fi

# Push the monorepo
cd "$MONOREPO"
git remote remove origin 2>/dev/null || true
git remote add origin "$REMOTE"
echo "Pushing to $REMOTE ..."
git push --force origin main

# Verify
echo "Push complete. Verify:"
git log --oneline -1
