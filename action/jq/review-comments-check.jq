def prefix: $ENV.PREFIX // "";
def footer(rule): "\n\n---\n<sub>\ud83c\udf3f <a href=\"https://docs.fallow.tools/explanations/dead-code#" + rule + "\">" + rule + "</a> \u00b7 Add <code>// fallow-ignore-next-line</code> to suppress</sub>";
[
  (.unused_files[]? | {
    path: (prefix + .path),
    line: 1,
    body: ":warning: **Unused file**\n\nThis file is not imported by any module and is unreachable from all entry points.\n\n<details>\n<summary>Why this matters</summary>\n\nUnused files increase project size, slow down tooling, and add maintenance burden. They can confuse developers who might think the code is still in use.\n</details>\n\n**Action:** Delete this file or import it where needed.\(footer("unused-files"))"
  }),
  (.unused_exports[]? | {
    path: (prefix + .path),
    line: .line,
    body: ":warning: **Unused \(if .is_type_only then "type " else "" end)export**\n\n\(if .is_re_export then "Re-exported" else "Exported" end) \(if .is_type_only then "type" else "value" end) `\(.export_name)` is never imported by other modules.\n\n<details>\n<summary>Why this matters</summary>\n\nUnused exports increase the public API surface, making it harder to refactor safely. They also prevent tree-shaking from removing dead code from production bundles.\n</details>\n\n**Action:** Remove the `export` keyword or delete the declaration.\n\n> If this is part of a public API, add it to the [entry configuration](https://docs.fallow.tools/configuration/overview).\(footer("unused-exports"))"
  }),
  (.unused_types[]? | {
    path: (prefix + .path),
    line: .line,
    body: ":warning: **Unused type export**\n\n\(if .is_re_export then "Re-exported" else "Exported" end) type `\(.export_name)` is never imported by other modules.\n\n**Action:** Remove the `export` keyword if only used internally.\(footer("unused-types"))"
  }),
  (.unused_dependencies[]? | {
    path: (prefix + .path),
    line: (if .line > 0 then .line else 1 end),
    body: ":warning: **Unused dependency**\n\nPackage `\(.package_name)` is listed in `\(.location)` but never imported anywhere in the project.\n\n```sh\nnpm uninstall \(.package_name)\n```\n\n<details>\n<summary>Why this matters</summary>\n\nUnused dependencies increase install time, bundle size, and attack surface. They also create noise in security audits.\n</details>\(footer("unused-dependencies"))"
  }),
  (.unused_dev_dependencies[]? | {
    path: (prefix + .path),
    line: (if .line > 0 then .line else 1 end),
    body: ":warning: **Unused devDependency**\n\nPackage `\(.package_name)` is listed in `devDependencies` but never imported.\n\n```sh\nnpm uninstall \(.package_name)\n```\(footer("unused-dependencies"))"
  }),
  (.unused_optional_dependencies[]? | {
    path: (prefix + .path),
    line: (if .line > 0 then .line else 1 end),
    body: ":warning: **Unused optionalDependency**\n\nPackage `\(.package_name)` is listed in `optionalDependencies` but never imported.\n\n```sh\nnpm uninstall \(.package_name)\n```\(footer("unused-dependencies"))"
  }),
  (.unused_enum_members[]? | {
    path: (prefix + .path),
    line: .line,
    body: ":warning: **Unused enum member**\n\n`\(.parent_name).\(.member_name)` is never referenced in the codebase.\n\n**Action:** Remove this member to keep the enum minimal.\n\n> Run `fallow fix` to auto-remove unused enum members.\(footer("unused-enum-members"))"
  }),
  (.unused_class_members[]? | {
    path: (prefix + .path),
    line: .line,
    body: ":warning: **Unused class member**\n\n`\(.parent_name).\(.member_name)` is never referenced.\n\n**Action:** Remove it or restrict visibility.\(footer("unused-class-members"))"
  }),
  (.unresolved_imports[]? | {
    path: (prefix + .path),
    line: .line,
    body: ":x: **Unresolved import**\n\nImport `\(.specifier)` could not be resolved to a file or package.\n\n**Check for:**\n- Typos in the import path\n- Missing dependency in `package.json`\n- Incorrect path alias in `tsconfig.json`\(footer("unresolved-imports"))"
  }),
  (.unlisted_dependencies[]? | (.package_name) as $pkg | .imported_from[]? | {
    path: (prefix + .path),
    line: .line,
    body: ":x: **Unlisted dependency**\n\nPackage `\($pkg)` is imported here but not declared in `package.json`. This will fail on a clean install.\n\n```sh\nnpm install \($pkg)\n```\(footer("unlisted-dependencies"))"
  }),
  (.duplicate_exports[]? | .locations as $locs | .locations[0] as $loc | {
    path: (prefix + $loc.path),
    line: $loc.line,
    body: ":warning: **Duplicate export**\n\nExport `\(.export_name)` is defined in \($locs | length) modules:\n\n\($locs | map("- `\(.path):\(.line)`") | join("\n"))\n\nThis causes ambiguity for consumers — barrel files may re-export the wrong one.\n\n**Action:** Keep one canonical location and remove the others.\(footer("duplicate-exports"))"
  }),
  (.circular_dependencies[]? | {
    path: (prefix + .files[0]),
    line: (if .line > 0 then .line else 1 end),
    body: ":warning: **Circular dependency**\n\nCircular import chain detected:\n\n```\n\(.files | join(" \u2192 ")) \u2192 \(.files[0])\n```\n\n<details>\n<summary>Why this matters</summary>\n\nCircular dependencies can cause:\n- `undefined` values at runtime due to incomplete module initialization\n- Unpredictable behavior depending on import order\n- Difficulty reasoning about data flow\n</details>\n\n**Action:** Extract shared logic into a separate module that both files can import.\(footer("circular-dependencies"))"
  }),
  (.type_only_dependencies[]? | {
    path: (prefix + .path),
    line: (if .line > 0 then .line else 1 end),
    body: ":blue_book: **Type-only dependency**\n\nPackage `\(.package_name)` is only used via `import type`. It doesn't need to be a production dependency.\n\n**Action:** Move from `dependencies` to `devDependencies`:\n\n```sh\nnpm uninstall \(.package_name) && npm install -D \(.package_name)\n```\(footer("type-only-dependencies"))"
  })
] | .[:($ENV.MAX | tonumber)]
