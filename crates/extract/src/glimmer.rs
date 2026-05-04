//! Helpers for Glimmer component files (`.gts` / `.gjs`).

use std::path::Path;

/// Return `true` for Glimmer source files.
#[must_use]
pub fn is_glimmer_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "gts" || ext == "gjs")
}

/// Blank Glimmer `<template>` blocks while preserving byte offsets and line
/// numbers for the JavaScript/TypeScript parser.
#[must_use]
pub fn strip_glimmer_templates(source: &str) -> Option<String> {
    let mut bytes = source.as_bytes().to_vec();
    let mut cursor = 0;
    let mut changed = false;

    while let Some(relative_start) = source[cursor..].find("<template") {
        let start = cursor + relative_start;
        let end = source[start..]
            .find("</template>")
            .map_or(source.len(), |relative_end| {
                start + relative_end + "</template>".len()
            });

        for byte in &mut bytes[start..end] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }

        changed = true;
        cursor = end;
        if cursor >= source.len() {
            break;
        }
    }

    if changed {
        String::from_utf8(bytes).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_template_blocks_and_preserves_newlines() {
        let source =
            "import x from './x';\n<template>\n  <x />\n</template>\nexport const y = x;\n";
        let stripped = strip_glimmer_templates(source).expect("template should be stripped");

        assert!(stripped.contains("import x from './x';"));
        assert!(stripped.contains("export const y = x;"));
        assert!(!stripped.contains("<template>"));
        assert_eq!(stripped.len(), source.len());
        assert_eq!(stripped.lines().count(), source.lines().count());
    }
}
