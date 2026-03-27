def prefix: $ENV.PREFIX // "";
def root: $ENV.FALLOW_ROOT // ".";
def rel_path: if startswith("/") then (. as $p | root as $r | if ($p | test("/\($r)/")) then ($p | capture("/\($r)/(?<rest>.*)") | .rest) else ($p | split("/") | .[-3:] | join("/")) end) else . end;
def footer: "\n\n---\n<sub>\ud83c\udf3f <a href=\"https://docs.fallow.tools/explanations/health\">complexity</a> \u00b7 Configure thresholds in <code>.fallowrc.json</code></sub>";
(.summary.max_cyclomatic_threshold // 20) as $cyc_t |
(.summary.max_cognitive_threshold // 15) as $cog_t |
[
  (.findings[]? | {
    path: (prefix + (.path | rel_path)),
    line: .line,
    body: ":warning: **High complexity**\n\nFunction `\(.name)` exceeds complexity thresholds:\n\n| Metric | Value | Threshold | Status |\n|:-------|------:|----------:|:------:|\n| Cyclomatic | **\(.cyclomatic)** | \($cyc_t) | \(if .exceeded == "cyclomatic" or .exceeded == "both" then ":red_circle:" else ":white_check_mark:" end) |\n| Cognitive | **\(.cognitive)** | \($cog_t) | \(if .exceeded == "cognitive" or .exceeded == "both" then ":red_circle:" else ":white_check_mark:" end) |\n| Lines | \(.line_count) | — | — |\n\n<details>\n<summary>What these metrics mean</summary>\n\n- **Cyclomatic complexity** counts independent code paths (branches, loops, conditions). High values mean many paths to test.\n- **Cognitive complexity** measures how hard code is to understand. Deep nesting, `break`/`continue`, and interleaved logic increase it.\n</details>\n\n**Action:** Split into smaller, focused functions. Consider extracting conditionals into well-named helpers.\(footer)"
  }),
  ((.targets // .refactoring_targets // [])[:5][]? | {
    path: (prefix + (.path | rel_path)),
    line: 1,
    body: ":bulb: **Refactoring target**\n\n| Priority | Effort | Confidence |\n|:---------|:-------|:-----------|\n| \(.priority) | \(.effort) | \(.confidence) |\n\n\(.recommendation)\n\n\(if .factors then "**Contributing factors:**\n\(.factors | map("- `\(.metric)`: \(.detail // (.value | tostring))") | join("\n"))\n" else "" end)\(if .evidence then "\n<details>\n<summary>Evidence</summary>\n\n\(if .evidence.unused_exports then "Unused exports: " + (.evidence.unused_exports | map("`\(.)`") | join(", ")) + "\n" else "" end)\n</details>\n" else "" end)\(footer)"
  })
] | .[:($ENV.MAX | tonumber)]
