#!/usr/bin/env bash
set -eo pipefail

# Install fallow binary via npm.
# Optional env: FALLOW_VERSION, INPUT_ROOT, FALLOW_INSTALL_DRY_RUN.

trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

is_safe_version_spec() {
  local spec
  spec="$(trim "$1")"
  if [ "$spec" = "latest" ]; then
    return 0
  fi
  local start_re='^[0-9xX*~^<>=]'
  local safe_re='^[0-9A-Za-z.*~^<>=| -]+$'
  # Accept semver versions and ranges, while rejecting protocols, paths, and
  # package aliases such as file:, link:, workspace:, git URLs, or /tmp/foo.
    [[ "$spec" =~ $start_re ]] &&
    [[ "$spec" =~ $safe_re ]] &&
    [[ ! "$spec" =~ : ]] &&
    [[ ! "$spec" =~ / ]] &&
    [[ ! "$spec" =~ [[:space:]]-[A-Za-z] ]]
}

is_exact_version() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][a-zA-Z0-9.]+)?$ ]]
}

project_fallow_spec() {
  local package_json="$1/package.json"
  if [ ! -f "$package_json" ]; then
    return 0
  fi

  node - "$package_json" <<'NODE'
const fs = require("node:fs");
const packageJson = process.argv[2];
const pkg = JSON.parse(fs.readFileSync(packageJson, "utf8"));
for (const section of ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"]) {
  const spec = pkg[section]?.fallow;
  if (typeof spec === "string" && spec.trim()) {
    console.log(spec.trim());
    process.exit(0);
  }
}
NODE
}

requested_version="$(trim "${FALLOW_VERSION:-}")"
root="${INPUT_ROOT:-.}"
project_spec="$(project_fallow_spec "$root" 2>/dev/null || true)"
project_spec="$(trim "$project_spec")"
install_spec=""

if [ -n "$requested_version" ]; then
  install_spec="$requested_version"
  echo "::notice::Using fallow version from action input: ${install_spec}"
elif [ -n "$project_spec" ]; then
  if is_safe_version_spec "$project_spec"; then
    install_spec="$project_spec"
    echo "::notice::Using fallow version from ${root}/package.json: ${install_spec}"
  else
    echo "::warning::Ignoring unsupported fallow package.json spec '${project_spec}'. Use a semver version or range, or set the action 'version' input explicitly."
    install_spec="latest"
  fi
else
  install_spec="latest"
fi

if ! is_safe_version_spec "$install_spec"; then
  echo "::error::Invalid version specifier: ${install_spec}. Use 'latest' or a semver version/range like '2.52.2' or '^2.52.0'."
  exit 2
fi

if [ "$install_spec" = "latest" ]; then
  install_arg="fallow"
else
  install_arg="fallow@${install_spec}"
fi

if [ "${FALLOW_INSTALL_DRY_RUN:-}" = "true" ]; then
  echo "DRY RUN: npm install -g ${install_arg}"
  exit 0
fi

npm install -g "$install_arg"

installed_version="$(fallow --version 2>/dev/null || echo 'unknown version')"
echo "Installed fallow ${installed_version}"

if [ -z "$requested_version" ] && [ -n "$project_spec" ] && is_exact_version "$project_spec"; then
  installed_semver="$(printf '%s\n' "$installed_version" | grep -Eo '[0-9]+\.[0-9]+\.[0-9]+([-.][a-zA-Z0-9.]+)?' | head -n 1 || true)"
  if [ -n "$installed_semver" ] && [ "$installed_semver" != "$project_spec" ]; then
    echo "::warning::Installed fallow ${installed_semver}, but ${root}/package.json pins ${project_spec}. Set the action 'version' input or align package.json to keep local and CI results comparable."
  fi
fi
