def count(obj; key): obj | if . then .[key] // 0 else 0 end;

(count(.check; "total_issues")) as $check_issues |
(count(.dupes.stats; "clone_groups")) as $dupes_groups |
(count(.health.summary; "functions_above_threshold")) as $health_findings |
($check_issues + $dupes_groups + $health_findings) as $total |

if $total == 0 then
  "# Fallow — Codebase Analysis\n\n" +
  "> [!NOTE]\n> **No issues found**\n\n" +
  "Dead code: clean \u00b7 Duplication: clean \u00b7 Complexity: clean"
else
  "# Fallow — Codebase Analysis\n\n" +
  "> [!WARNING]\n> **\($total) issues** found\n\n" +
  "| Analysis | Issues |\n|----------|-------:|\n" +
  (if $check_issues > 0 then "| Dead code | \($check_issues) |\n" else "" end) +
  (if $dupes_groups > 0 then "| Duplication | \($dupes_groups) clone groups |\n" else "" end) +
  (if $health_findings > 0 then "| Complexity | \($health_findings) functions above threshold |\n" else "" end) +
  "\n> Run `fallow dead-code`, `fallow dupes`, or `fallow health` individually for detailed findings."
end
