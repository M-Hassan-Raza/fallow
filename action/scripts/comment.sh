#!/usr/bin/env bash
set -euo pipefail

# Post or update a PR comment with analysis results
# Required env: GH_TOKEN, PR_NUMBER, GH_REPO, FALLOW_COMMAND, ACTION_JQ_DIR

# Select jq script
case "$FALLOW_COMMAND" in
  dead-code|check) JQ_FILE="${ACTION_JQ_DIR}/summary-check.jq" ;;
  dupes)           JQ_FILE="${ACTION_JQ_DIR}/summary-dupes.jq" ;;
  health)          JQ_FILE="${ACTION_JQ_DIR}/summary-health.jq" ;;
  fix)             JQ_FILE="${ACTION_JQ_DIR}/summary-fix.jq" ;;
  "")              JQ_FILE="${ACTION_JQ_DIR}/summary-combined.jq" ;;
  *)               echo "::error::Unexpected command: ${FALLOW_COMMAND}"; exit 2 ;;
esac

# Generate comment body
if ! COMMENT_BODY="$(jq -r -f "$JQ_FILE" fallow-results.json)

<!-- fallow-results -->"; then
  echo "::warning::Failed to generate PR comment body"
  exit 0
fi

# Find existing fallow comment to update (avoids spam on busy PRs)
COMMENT_ID=$(gh api \
  --paginate \
  "repos/${GH_REPO}/issues/${PR_NUMBER}/comments" \
  --jq '.[] | select(.body | contains("<!-- fallow-results -->")) | .id' \
  2>/dev/null | head -1)

if [ -n "$COMMENT_ID" ]; then
  if ! gh api \
    "repos/${GH_REPO}/issues/comments/${COMMENT_ID}" \
    --method PATCH \
    --field body="$COMMENT_BODY" \
    > /dev/null; then
    echo "::warning::Failed to update PR comment"
  else
    echo "Updated existing PR comment"
  fi
else
  if ! gh api \
    "repos/${GH_REPO}/issues/${PR_NUMBER}/comments" \
    --method POST \
    --field body="$COMMENT_BODY" \
    > /dev/null; then
    echo "::warning::Failed to create PR comment"
  else
    echo "Created new PR comment"
  fi
fi
