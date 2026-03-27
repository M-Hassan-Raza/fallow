def prefix: $ENV.PREFIX // "";
def root: $ENV.FALLOW_ROOT // ".";
def rel_path: if startswith("/") then (. as $p | root as $r | if ($p | test("/\($r)/")) then ($p | capture("/\($r)/(?<rest>.*)") | .rest) else ($p | split("/") | .[-3:] | join("/")) end) else . end;
def short_path: split("/") | if length > 3 then .[-3:] | join("/") else join("/") end;
def footer: "\n\n---\n<sub>\ud83c\udf3f <a href=\"https://docs.fallow.tools/explanations/duplication\">code-duplication</a> \u00b7 Add <code>// fallow-ignore-next-line</code> to suppress</sub>";
[
  (.clone_families // [])[] | . as $family |
    ($family.suggestions // []) as $suggestions |
    $family.groups[]? | . as $group |
    ($group.instances | length) as $count |
    .instances[]? | . as $inst |
      ($group.instances | map(select(. != $inst)) | map("- `\(.file | rel_path):\(.start_line)-\(.end_line)`") | join("\n")) as $others |
      {
        path: (prefix + ($inst.file | rel_path)),
        start_line: $inst.start_line,
        line: $inst.end_line,
        body: ":warning: **Code duplication**\n\n**\($group.line_count) duplicated lines** \u00b7 \($group.token_count) tokens \u00b7 \($count) instances\n\nAlso found in:\n\($others)\n\n\(if $inst.fragment then "<details>\n<summary>View duplicated code</summary>\n\n```ts\n\($inst.fragment[:800])\n```\n</details>\n\n" else "" end)\(if ($suggestions | length) > 0 then ($suggestions | map(":bulb: **\(.kind):** \(.description)\(if .estimated_savings then " (\(.estimated_savings) lines saved)" else "" end)") | join("\n")) + "\n" else "**Action:** Extract a shared function to eliminate this duplication.\n" end)\(footer)"
      }
] | .[:($ENV.MAX | tonumber)]
