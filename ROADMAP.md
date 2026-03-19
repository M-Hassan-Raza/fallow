# Fallow Roadmap

> Last updated: 2026-03-19

Fallow is a Rust-native dead code and duplication analyzer for JavaScript/TypeScript — the fast alternative to knip and jscpd.

**Two pillars**: Dead code analysis (`fallow check`) and duplication detection (`fallow dupes`) are co-equal. Every phase advances both.

---

## Current State (v0.3.x)

### Dead Code (`check`)
- **10 issue types**: unused files, exports, types, dependencies, devDeps, enum members, class members, unresolved imports, unlisted deps, duplicate exports
- **46 framework plugins**: declarative glob patterns + AST-based config parsing for ~20 plugins (15 with rich config extraction)
- **Deep config parsing** for all top 10 frameworks: ESLint, Vite, Jest, Storybook, Tailwind, Webpack, TypeScript, Babel, Rollup, PostCSS — extracts entry points, dependencies, setup files, and tooling references from config objects via Oxc AST analysis (no JS runtime)
- **Non-JS file support**: Vue/Svelte SFC (`<script>` block extraction with HTML comment filtering, `lang="ts"`/`lang="tsx"`, `<script src="...">`), Astro (frontmatter), MDX (import/export statements), CSS/SCSS (`@import`, `@use`, `@forward`, `@apply`/`@tailwind` as Tailwind dependency usage)
- **4 output formats**: human, JSON, SARIF, compact
- **Auto-fix**: remove unused exports and dependencies (`fix --dry-run` to preview)
- **CI features**: `--changed-since`, `--baseline`/`--save-baseline`, `--fail-on-issues`, SARIF for GitHub Code Scanning
- **Rules system**: per-issue-type severity (`error`/`warn`/`off`) in config. All 10 issue types configurable. `--fail-on-issues` promotes `warn` → `error`
- **Inline suppression**: `// fallow-ignore-next-line [issue-type]` and `// fallow-ignore-file [issue-type]` comments, supporting all issue types including `code-duplication`
- **Production mode**: `--production` flag excludes test/dev files, limits to production scripts, skips devDep warnings, reports type-only imports in production deps
- **Script parser**: extracts binary names (mapped to packages), `--config` args (entry points), file path args from `package.json` scripts; handles env wrappers and package manager runners

### Duplication (`dupes`)
- **4 detection modes**: strict (exact tokens), mild (normalized syntax), weak (different literals), semantic (renamed variables)
- **Suffix array with LCP**: no quadratic pairwise comparison — 10x+ faster than jscpd (up to 500x on large projects)
- **Clone families**: groups clone groups sharing the same file set with refactoring suggestions (extract function/module)
- **Baseline tracking**: `--save-baseline` / `--baseline` for incremental CI adoption of duplication thresholds
- **Filtering**: `--skip-local`, `--threshold`, `--min-lines`, `--min-tokens`, `duplicates.ignore` config globs
- **Cross-language clone detection**: `--cross-language` strips TypeScript type annotations for `.ts` ↔ `.js` matching
- **Configurable normalization**: fine-grained overrides (`ignore_identifiers`, `ignore_string_values`, `ignore_numeric_values`) on top of detection mode defaults
- **Dead code × duplication cross-reference**: `check --include-dupes` identifies clone instances in unused files or overlapping unused exports as combined high-priority findings

### Shared Infrastructure
- **CLI commands**: check, dupes, watch, fix, init, list, schema, config-schema, migrate
- **Config format**: JSONC (default), JSON, TOML — with `$schema` support for IDE autocomplete/validation
- **LSP server**: diagnostics for all 10 dead code issue types + quick-fix code actions
- **VS Code extension**: tree views for dead code and duplicates, status bar, one-click fixes, auto-download of LSP binary
- **MCP server**: stdio transport, exposes analyze/check_changed/find_dupes/fix_preview/fix_apply/project_info tools
- **GitHub Action**: SARIF upload, PR comments, configurable thresholds, baseline support
- **Debug & trace tooling**: `--trace FILE:EXPORT`, `--trace-file PATH`, `--trace-dependency PACKAGE`, `dupes --trace FILE:LINE`, `--performance`
- **External plugins**: community-driven `fallow-plugin-*.toml` definitions with `docs/plugin-authoring.md` guide
- **Migration**: `fallow migrate` reads knip/jscpd config and generates fallow config
- **Performance**: rayon parallelism, oxc_parser, incremental bincode cache, flat graph storage, DashMap lock-free bare specifier cache
- **Duplication accuracy**: curated benchmark corpus with 100% precision/recall on default settings

### Known Limitations

- **Syntactic analysis only**: No TypeScript type information. Projects using `isolatedModules: true` (required for esbuild/swc/vite) are well-served; legacy tsc-only projects may see false positives on type-only imports.
- **Config parsing ceiling**: AST-based extraction covers static object literals, string arrays, and simple wrappers like `defineConfig(...)`. Computed values (`getPlugins()`), conditionals (`process.env.NODE_ENV`), and nested config factories are out of reach without JS eval.
- **CSS/SCSS parsing is regex-based**: Handles `@import`, `@use`, `@forward`, `@apply`, `@tailwind` with comment stripping, but does not parse full CSS syntax. CSS Modules (`.module.css` class name exports) are not yet tracked. SCSS partials (`_variables.scss` from `@use "variables"`) rely on the resolver, not SCSS-specific partial resolution.

---

## Phase 1: Trustworthy Results (v0.4.0)

The goal: a developer can run `fallow check` on a real project and get results they trust. This is the gate to 1.0.

### 1.1 Cross-Workspace Resolution

**Dealbreaker for monorepo adoption.** Build a unified module graph across all workspace packages:
- Resolve cross-workspace imports via `node_modules` symlinks, `package.json` `exports` field, and tsconfig project references
- Handle pnpm's content-addressable store: detect `.pnpm` paths and map them back to workspace sources
- A single `fallow check` at the workspace root analyzes all packages together
- `--workspace <name>` flag scopes output to one package while keeping the full graph

**Architecture note**: This requires a `ProjectState` struct that owns the module graph, file registry, and resolved modules across workspace boundaries. This also requires stable FileIds — the current `FileId(idx as u32)` assigned by sort order re-indexes everything when files are added/removed. Introduce `ProjectState` with stable ID assignment here — it also unblocks incremental analysis later.

### 1.2 Large-Scale Benchmarks

Add benchmarks on 1,000+ and 5,000+ file projects for both `check` and `dupes`. Show warm cache vs cold. Publish methodology, hardware specs, and memory usage. The current 3-project, 174-286 file suite doesn't substantiate claims for real adoption.

---

## Phase 2: Editor Experience (v0.5.0)

### 2.1 Incremental Analysis

**Two-phase approach** (per Rust architect review):

**Phase A (cheap incremental)**: Re-parse only changed files, rebuild the full graph. Graph construction is sub-millisecond; parsing is the bottleneck. This gets 80% of the benefit with 20% of the work.

**Phase B (fine-grained incremental, post-1.0)**: Patch the graph in place, track export-level dependencies, incremental re-export chain propagation. This requires redesigning the flat `Vec<Edge>` storage to support insertion/removal.

### 2.2 Enhanced Code Actions & Code Lens

- Usage counts on exports (code lens)
- "Remove unused export", "Delete unused file", "Remove unused dependency"
- "Extract duplicate" — for duplication: offer to extract a clone family into a shared function
- Hover: show where an export is used, or show other locations of a duplicate block

---

## 1.0: Stable & Trustworthy

**1.0 criteria** — not a feature milestone, a quality milestone:

- [ ] Trustworthy results on the top 20 JS/TS project archetypes (Next.js, Vite, monorepo, NestJS, React Native)
- [ ] Cross-workspace resolution works for npm, yarn, and pnpm workspaces
- [ ] Stable config format with backwards compatibility promise
- [ ] Stable JSON output schema for CI consumers
- [ ] Large-scale benchmarks published (1000+ files, warm/cold cache, memory)
- [ ] Migration guide from knip with worked examples

---

## Post-1.0: Exploration

These are ideas, not commitments. They ship as 1.x releases based on user demand.

### CSS Modules Support
Track CSS Module class names (`.module.css`) as named exports, so `import styles from './Button.module.css'` + `styles.button` marks `.button` as used. Requires CSS class name extraction and mapping to JS import destructuring.

### Historical Trend Tracking
Store baselines over time. Generate trend reports for both dead code and duplication: "dead code grew 15% this quarter, duplication dropped 3%." Dashboard-friendly JSON API.

### Intelligent Grouping
Group related dead code (e.g., an unused feature spanning 5 files). For duplication: suggest bulk refactors for clone families that share a common abstraction opportunity.

### Additional Reporters
Markdown (PR comments), Code Climate, Codeowners integration. Custom reporters via external binary.

### More Auto-Fix Targets
Remove unused enum/class members, delete unused files (`--allow-remove-files`), `--fix-type` flag, post-fix formatting integration.

### JSDoc/TSDoc Tag Support
`@public` (never report as unused), `@internal` (only report if unused within project), custom tags.

### Supply Chain Security
Sigstore binary signing, SBOM generation, reproducible builds. Important for enterprise adoption but not an adoption blocker today.

### Cloud & Hosted Features
Remote analysis cache, trend dashboard, team management. Details TBD based on adoption and demand.

---

## Community & Adoption (ongoing, not phased)

These are not gated on any release — they should happen continuously:

- **Documentation site**: Move from GitHub wiki to a proper docs site (Starlight, Nextra, or similar)
- **CHANGELOG**: Maintain a changelog from v0.3 onward
- **Communication**: GitHub Discussions for support, feedback, and RFCs
- **Contributing guide**: Plugin authoring tutorial, "your first PR" guide, issue templates
- **Compatibility matrix**: For each of the top 20 frameworks, document exactly what fallow detects vs. knip — let users make informed choices
- **Blog posts**: Technical deep-dives on the suffix array algorithm, the Oxc parser integration, benchmark methodology
- **Backwards compatibility policy**: State explicitly how config format and JSON output changes are handled across versions

---

## Why This Matters

JavaScript/TypeScript codebases accumulate dead code and duplication faster than any other ecosystem — broad dependency trees, rapid framework churn, and copy-paste-driven development create entropy at scale. AI-assisted development accelerates this: agents generate code but rarely suggest deletions, and code clones have grown significantly since AI assistants became prevalent.

Fallow should be fast enough to run on every save and every commit — not as a monthly audit, but as continuous feedback. The combination of dead code analysis and duplication detection in a single sub-second tool means one integration covers both problems.

---

## Release Milestones

| Version | Theme | Key Deliverables |
|---------|-------|-----------------|
| **0.2** | Foundation | 10 issue types, 40 plugins, 4 duplication modes, clone families, LSP, CI features, rules system |
| **0.3** | Reach | 46 plugins, CSS/SCSS support, cross-language dupes, MCP server, VS Code extension, GitHub Action, trace tooling, external plugins, migrate command |
| **0.4** | Trust | Cross-workspace resolution, large-scale benchmarks |
| **0.5** | Editor | Incremental analysis, code lens, enhanced code actions |
| **1.0** | Stable | Quality milestone — trustworthy results, stable formats, full docs |
