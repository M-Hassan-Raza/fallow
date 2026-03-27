#!/usr/bin/env bash
set -euo pipefail

# Install fallow binary via npm
# Required env: FALLOW_VERSION

# Validate version to prevent npm URL/path injection
if ! [[ "$FALLOW_VERSION" =~ ^(latest|[0-9]+\.[0-9]+\.[0-9]+([-.][a-zA-Z0-9.]+)*)$ ]]; then
  echo "::error::Invalid version specifier: ${FALLOW_VERSION}. Use 'latest' or a semver like '0.3.0'."
  exit 2
fi

if [ "$FALLOW_VERSION" = "latest" ]; then
  npm install -g fallow
else
  npm install -g "fallow@${FALLOW_VERSION}"
fi
echo "Installed fallow $(fallow --version 2>/dev/null || echo 'unknown version')"
