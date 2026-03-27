def san: gsub("\n"; " ") | gsub("\r"; " ") | gsub("%"; "%25");
def nl: "%0A";
(.summary.max_cyclomatic_threshold // 20) as $cyc_t |
(.summary.max_cognitive_threshold // 15) as $cog_t |
[
  (.findings[]? |
    if .exceeded == "both" then
      "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=High complexity::Function '\(.name | san)' exceeds both complexity thresholds:\(nl)\(nl)  \u2022 Cyclomatic: \(.cyclomatic) (threshold: \($cyc_t))\(nl)  \u2022 Cognitive: \(.cognitive) (threshold: \($cog_t))\(nl)  \u2022 Lines: \(.line_count)\(nl)\(nl)Consider splitting this function into smaller, focused functions."
    elif .exceeded == "cyclomatic" then
      "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=High cyclomatic complexity::Function '\(.name | san)' has \(.cyclomatic) code paths (threshold: \($cyc_t)).\(nl)\(nl)  \u2022 Cyclomatic: \(.cyclomatic)\(nl)  \u2022 Cognitive: \(.cognitive)\(nl)  \u2022 Lines: \(.line_count)\(nl)\(nl)High cyclomatic complexity means many branches to test.\(nl)Consider extracting conditionals or using early returns."
    else
      "::warning file=\(.path | san),line=\(.line),col=\(.col + 1),title=High cognitive complexity::Function '\(.name | san)' is hard to understand (cognitive: \(.cognitive), threshold: \($cog_t)).\(nl)\(nl)  \u2022 Cyclomatic: \(.cyclomatic)\(nl)  \u2022 Cognitive: \(.cognitive)\(nl)  \u2022 Lines: \(.line_count)\(nl)\(nl)High cognitive complexity means deeply nested or interleaved logic.\(nl)Consider flattening control flow or extracting helper functions."
    end),
  ((.targets // .refactoring_targets // [])[:5][]? |
    "::notice file=\(.path | san),title=Refactoring target (\(.effort) effort)::Priority: \(.priority) | Confidence: \(.confidence)\(nl)\(nl)\(.recommendation | san)\(nl)\(nl)\(if .factors then (.factors | map("  \u2022 \(.metric): \(.detail // (.value | tostring))") | join(nl)) else "" end)")
] | .[]
