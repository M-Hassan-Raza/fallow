#!/usr/bin/env bash
set -eo pipefail

# Write job summary using the appropriate jq script
# Required env: FALLOW_COMMAND, ACTION_JQ_DIR

select_summary_script() {
  case "$FALLOW_COMMAND" in
    dead-code|check) echo "${ACTION_JQ_DIR}/summary-check.jq" ;;
    dupes)           echo "${ACTION_JQ_DIR}/summary-dupes.jq" ;;
    health)          echo "${ACTION_JQ_DIR}/summary-health.jq" ;;
    fix)             echo "${ACTION_JQ_DIR}/summary-fix.jq" ;;
    "")              echo "${ACTION_JQ_DIR}/summary-combined.jq" ;;
    *)               echo "::error::Unexpected command: ${FALLOW_COMMAND}"; exit 2 ;;
  esac
}

JQ_FILE=$(select_summary_script)
jq -r -f "$JQ_FILE" fallow-results.json >> "$GITHUB_STEP_SUMMARY"
