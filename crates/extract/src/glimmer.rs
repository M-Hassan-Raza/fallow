//! Helpers for Glimmer component files (`.gts` / `.gjs`).

use std::path::Path;

/// Return `true` for Glimmer source files.
#[must_use]
pub fn is_glimmer_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "gts" || ext == "gjs")
}

/// Strip Glimmer `<template>` blocks while preserving byte offsets and line
/// numbers for the JavaScript/TypeScript parser.
///
/// Two replacement strategies are used based on the surrounding context, so
/// that the post-strip source is always syntactically valid TS/JS regardless
/// of how many templates appear in the file:
///
/// - Class-body context (`<template>` directly inside a class body): blank
///   all bytes to spaces, leaving an empty class body, which is valid
///   TS/JS. This matches the original behavior.
///
/// - Expression context (`<template>` follows `=`, `,`, `(`, `?`, or `:`):
///   replace with a byte-length-preserving parenthesised template literal
///   `` (`...`) ``. The opening tag's bytes become `` (` `` plus spaces;
///   the closing tag's bytes become spaces plus `` `) ``. Content bytes
///   that would interfere with the inert string (`` ` ``, `$`, `\`) are
///   escaped to spaces. This keeps the surrounding statement
///   (e.g. `const x = <template>...</template>;`) syntactically valid so
///   oxc can extract imports and exports from the rest of the file.
///
/// Without the expression-context strategy, a file containing both a
/// module-level template expression and a class-body template would yield
/// `const x = ;` after blanking, which is a syntax error that prevents oxc
/// from recovering any imports.
#[must_use]
pub fn strip_glimmer_templates(source: &str) -> Option<String> {
    let mut bytes = source.as_bytes().to_vec();
    let mut cursor = 0;
    let mut changed = false;
    let n = source.len();

    while let Some(relative_start) = source[cursor..].find("<template") {
        let start = cursor + relative_start;
        // End of opening tag: position just after the first `>` that follows
        // `<template`. Scanning past the `<template` word avoids treating a
        // `>` inside the literal substring as a tag close, and absorbs any
        // attributes the opening tag may carry.
        let after_template_word = start + "<template".len();
        let opening_end = source[after_template_word..]
            .find('>')
            .map_or(n, |r| after_template_word + r + 1);
        // Find the matching closing tag.
        let close_relative = source[opening_end..].find("</template>");
        let (close_start_abs, close_end) = match close_relative {
            Some(r) => (opening_end + r, opening_end + r + "</template>".len()),
            None => (n, n),
        };

        let opening_len = opening_end - start;
        let closing_len = close_end - close_start_abs;
        let in_expr_position = is_expression_position(source.as_bytes(), start);
        let can_use_expr_form =
            in_expr_position && close_relative.is_some() && opening_len >= 2 && closing_len >= 2;

        if can_use_expr_form {
            // Opening tag: `(` `` ` `` followed by spaces, preserving any
            // newlines that may live inside a multi-line opening tag.
            bytes[start] = b'(';
            bytes[start + 1] = b'`';
            for byte in &mut bytes[start + 2..opening_end] {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
            // Content: escape characters that would otherwise interpolate or
            // terminate the template literal.
            for byte in &mut bytes[opening_end..close_start_abs] {
                if matches!(*byte, b'`' | b'$' | b'\\') {
                    *byte = b' ';
                }
            }
            // Closing tag: spaces, then `` ` `` `)`. Newlines preserved.
            for byte in &mut bytes[close_start_abs..close_end - 2] {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
            bytes[close_end - 2] = b'`';
            bytes[close_end - 1] = b')';
        } else {
            // Fallback: blank everything to spaces (preserves newlines).
            // Valid for class-body templates and for unclosed templates.
            for byte in &mut bytes[start..close_end] {
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
            }
        }

        changed = true;
        cursor = close_end;
        if cursor >= n {
            break;
        }
    }

    if changed {
        String::from_utf8(bytes).ok()
    } else {
        None
    }
}

/// Return `true` when the byte at `pos` opens a template in JS/TS expression
/// position. Heuristic: walk back over whitespace and check the previous
/// non-whitespace byte against a small set of expression-only delimiters.
///
/// Covered shapes:
/// - assignment / declaration initializer: `const x = <template>...`
/// - argument: `foo(<template>...)`, `decorator(<template>...)`
/// - sequence expression: `(a, <template>...)`
/// - ternary: `cond ? <template>... : <template>...`
///
/// Class-body templates land on `{`, `;`, or `}` (after a prior method
/// body), none of which appear in the set, so they fall through to the
/// blank-out branch.
fn is_expression_position(bytes: &[u8], pos: usize) -> bool {
    let prev = bytes[..pos]
        .iter()
        .rev()
        .copied()
        .find(|b| !matches!(*b, b' ' | b'\t' | b'\n' | b'\r'));
    matches!(prev, Some(b'=' | b',' | b'(' | b'?' | b':'))
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

    #[test]
    fn strips_class_body_template_to_empty_class() {
        let source = "import Component from '@glimmer/component';\nexport default class X extends Component {\n  <template>billing</template>\n}\n";
        let stripped = strip_glimmer_templates(source).expect("template should be stripped");

        // Class-body context: blanked out (empty class body is valid).
        assert!(stripped.contains("class X extends Component {"));
        assert!(!stripped.contains("<template>"));
        assert!(!stripped.contains('('));
        assert_eq!(stripped.len(), source.len());
    }

    #[test]
    fn replaces_module_level_template_expression_with_parenthesised_literal() {
        let source = "const x = <template>foo</template>;\n";
        let stripped = strip_glimmer_templates(source).expect("template should be stripped");

        assert!(stripped.contains("const x = (`"));
        assert!(stripped.contains("`);"));
        assert!(!stripped.contains("<template>"));
        assert_eq!(stripped.len(), source.len());
    }

    #[test]
    fn handles_multi_template_module_and_class_in_same_file() {
        let source = "import C from '@glimmer/component';\nconst W = <template>\n  one\n</template>;\nexport default class X extends C {\n  <template>\n    two\n  </template>\n}\n";
        let stripped = strip_glimmer_templates(source).expect("templates should be stripped");

        // Module-level template expression becomes a parenthesised literal.
        assert!(stripped.contains("const W = (`"));
        assert!(stripped.contains("`);"));
        // Class-body template is blanked.
        assert!(stripped.contains("class X extends C {"));
        assert!(!stripped.contains("<template>"));
        assert!(!stripped.contains("</template>"));
        assert_eq!(stripped.len(), source.len());
        assert_eq!(stripped.lines().count(), source.lines().count());
    }

    #[test]
    fn escapes_backtick_dollar_backslash_inside_expression_template() {
        // Template content contains characters that would otherwise terminate
        // the template literal or trigger interpolation. We escape `` ` ``,
        // `$`, and `\` to spaces; the braces stay because once `$` is gone
        // the parser will not read `{c}` as an interpolation.
        let source = "const x = <template>a`b${c}d\\e</template>;\n";
        let stripped = strip_glimmer_templates(source).expect("template should be stripped");

        assert!(!stripped.contains('`') || stripped.matches('`').count() == 2);
        assert!(!stripped.contains("${"));
        assert!(!stripped.contains('\\'));
        assert!(stripped.contains("a b "));
        assert!(stripped.contains("d e"));
        assert_eq!(stripped.len(), source.len());
    }

    #[test]
    fn unclosed_template_blanks_to_eof_without_expression_form() {
        let source = "const x = <template>oops\nexport const y = 1;\n";
        let stripped = strip_glimmer_templates(source).expect("template should be stripped");

        // No `</template>`, so we cannot use the parenthesised literal form
        // (we have no place to close it). Fall back to blanking. The file is
        // malformed either way; the only requirement is that we do not panic.
        assert!(!stripped.contains("<template>"));
        assert_eq!(stripped.len(), source.len());
    }

    #[test]
    fn handles_template_after_typed_initializer() {
        // `=` is the previous non-whitespace byte even across the type annotation.
        let source = "const x: TOC<{}> = <template>hi</template>;\n";
        let stripped = strip_glimmer_templates(source).expect("template should be stripped");

        assert!(stripped.contains("const x: TOC<{}> = (`"));
        assert!(stripped.contains("`);"));
        assert_eq!(stripped.len(), source.len());
    }

    #[test]
    fn handles_template_in_decorator_call() {
        let source = "@Some(<template>x</template>)\nclass Foo {}\n";
        let stripped = strip_glimmer_templates(source).expect("template should be stripped");

        // Previous non-whitespace byte is `(`: expression form.
        assert!(stripped.contains("@Some((`"));
        assert!(stripped.contains("`))"));
        assert_eq!(stripped.len(), source.len());
    }
}
