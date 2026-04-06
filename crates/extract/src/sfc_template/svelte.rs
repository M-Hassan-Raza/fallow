use std::sync::LazyLock;

use rustc_hash::FxHashSet;

use crate::template_usage::TemplateUsage;

use super::scanners::{scan_curly_section, scan_html_tag};
use super::shared::{
    extract_pattern_binding_names, merge_component_tag_usage,
    merge_expression_usage_allow_dollar_refs, merge_statement_usage_allow_dollar_refs,
};

static STYLE_BLOCK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<style\b(?:[^>"']|"[^"]*"|'[^']*')*>(?P<body>[\s\S]*?)</style>"#)
        .expect("valid regex")
});

static SCRIPT_BLOCK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<script\b(?:[^>"']|"[^"]*"|'[^']*')*>(?P<body>[\s\S]*?)</script>"#)
        .expect("valid regex")
});

static HTML_COMMENT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<!--.*?-->").expect("valid regex"));

static SVELTE_EACH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?is)^#each\s+(?P<iterable>.+?)\s+as\s+(?P<bindings>.+?)(?:\s*\((?P<key>.+)\))?$",
    )
    .expect("valid regex")
});

static SVELTE_AWAIT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?is)^#await\s+(?P<expr>.+)$").expect("valid regex"));

static SVELTE_THEN_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^:then(?:\s+(?P<binding>.+))?$").expect("valid regex")
});

static SVELTE_CATCH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^:catch(?:\s+(?P<binding>.+))?$").expect("valid regex")
});

static SVELTE_SNIPPET_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^#snippet\s+[A-Za-z_$][\w$]*\s*\((?P<params>.*)\)\s*$")
        .expect("valid regex")
});

#[derive(Debug, Clone, PartialEq, Eq)]
enum SvelteBlockKind {
    Root,
    If,
    Each,
    Await,
    Key,
    Snippet,
    Element,
}

const VOID_HTML_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

#[derive(Debug, Clone)]
struct SvelteScopeFrame {
    kind: SvelteBlockKind,
    locals: Vec<String>,
}

pub(super) fn collect_template_usage(
    source: &str,
    imported_bindings: &FxHashSet<String>,
) -> TemplateUsage {
    if imported_bindings.is_empty() {
        return TemplateUsage::default();
    }

    let markup = strip_non_template_content(source);
    if markup.is_empty() {
        return TemplateUsage::default();
    }

    let mut usage = TemplateUsage::default();
    let mut scopes = vec![SvelteScopeFrame {
        kind: SvelteBlockKind::Root,
        locals: Vec::new(),
    }];

    let bytes = markup.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => {
                let Some((tag, next_index)) = scan_curly_section(&markup, index, 1, 1) else {
                    break;
                };
                apply_tag(tag.trim(), imported_bindings, &mut scopes, &mut usage);
                index = next_index;
            }
            b'<' => {
                let Some((tag, next_index)) = scan_html_tag(&markup, index) else {
                    break;
                };
                apply_markup_tag(tag, imported_bindings, &mut scopes, &mut usage);
                index = next_index;
            }
            _ => index += 1,
        }
    }

    usage
}

fn strip_non_template_content(source: &str) -> String {
    let mut hidden_ranges: Vec<(usize, usize)> = Vec::new();
    hidden_ranges.extend(
        HTML_COMMENT_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.extend(
        SCRIPT_BLOCK_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.extend(
        STYLE_BLOCK_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.sort_unstable_by_key(|range| range.0);

    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(hidden_ranges.len());
    for (start, end) in hidden_ranges {
        if let Some((_, last_end)) = merged.last_mut()
            && start <= *last_end
        {
            *last_end = (*last_end).max(end);
            continue;
        }
        merged.push((start, end));
    }

    let mut visible = String::new();
    let mut cursor = 0;
    for (start, end) in merged {
        if cursor < start {
            visible.push_str(&source[cursor..start]);
        }
        cursor = end;
    }
    if cursor < source.len() {
        visible.push_str(&source[cursor..]);
    }
    visible
}

fn apply_tag(
    tag: &str,
    imported_bindings: &FxHashSet<String>,
    scopes: &mut Vec<SvelteScopeFrame>,
    usage: &mut TemplateUsage,
) {
    if tag.is_empty() {
        return;
    }

    if let Some(rest) = tag.strip_prefix('/') {
        pop_scope(scopes, rest.trim());
        return;
    }

    if let Some(expr) = tag.strip_prefix("#if") {
        merge_expression_usage_allow_dollar_refs(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::If,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_EACH_RE.captures(tag) {
        let iterable = captures.name("iterable").map_or("", |m| m.as_str()).trim();
        let bindings = captures.name("bindings").map_or("", |m| m.as_str()).trim();
        let each_locals = extract_pattern_binding_names(bindings);
        let current = current_locals(scopes);
        merge_expression_usage_allow_dollar_refs(usage, iterable, imported_bindings, &current);
        if let Some(key) = captures.name("key").map(|m| m.as_str().trim())
            && !key.is_empty()
        {
            let mut key_locals = current;
            key_locals.extend(each_locals.iter().cloned());
            merge_expression_usage_allow_dollar_refs(usage, key, imported_bindings, &key_locals);
        }
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Each,
            locals: each_locals,
        });
        return;
    }

    if let Some(captures) = SVELTE_AWAIT_RE.captures(tag) {
        let expr = captures.name("expr").map_or("", |m| m.as_str()).trim();
        merge_expression_usage_allow_dollar_refs(
            usage,
            expr,
            imported_bindings,
            &current_locals(scopes),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Await,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_THEN_RE.captures(tag) {
        if let Some(frame) = scopes
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame.kind, SvelteBlockKind::Await))
        {
            frame.locals = captures
                .name("binding")
                .map(|m| extract_pattern_binding_names(m.as_str()))
                .unwrap_or_default();
        }
        return;
    }

    if let Some(captures) = SVELTE_CATCH_RE.captures(tag) {
        if let Some(frame) = scopes
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame.kind, SvelteBlockKind::Await))
        {
            frame.locals = captures
                .name("binding")
                .map(|m| extract_pattern_binding_names(m.as_str()))
                .unwrap_or_default();
        }
        return;
    }

    if let Some(expr) = tag.strip_prefix("#key") {
        merge_expression_usage_allow_dollar_refs(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Key,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_SNIPPET_RE.captures(tag) {
        let params = captures.name("params").map_or("", |m| m.as_str());
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Snippet,
            locals: extract_pattern_binding_names(params),
        });
        return;
    }

    if let Some(expr) = tag.strip_prefix("@html") {
        merge_expression_usage_allow_dollar_refs(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if let Some(expr) = tag.strip_prefix("@render") {
        merge_expression_usage_allow_dollar_refs(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if let Some(stmt) = tag.strip_prefix("@const") {
        let locals = current_locals(scopes);
        merge_statement_usage_allow_dollar_refs(usage, stmt.trim(), imported_bindings, &locals);
        if let Some(lhs) = stmt.split_once('=').map(|(lhs, _)| lhs.trim()) {
            let new_bindings = extract_pattern_binding_names(lhs);
            if let Some(frame) = scopes.last_mut() {
                frame.locals.extend(new_bindings);
            }
        }
        return;
    }

    if let Some(expr) = tag.strip_prefix("@debug") {
        merge_expression_usage_allow_dollar_refs(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if let Some(expr) = tag.strip_prefix(":else if") {
        merge_expression_usage_allow_dollar_refs(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if tag.starts_with(":else") {
        return;
    }

    merge_expression_usage_allow_dollar_refs(
        usage,
        tag,
        imported_bindings,
        &current_locals(scopes),
    );
}

fn apply_markup_tag(
    tag: &str,
    imported_bindings: &FxHashSet<String>,
    scopes: &mut Vec<SvelteScopeFrame>,
    usage: &mut TemplateUsage,
) {
    let trimmed = tag.trim();
    if trimmed.starts_with("</") {
        if let Some(frame) = scopes.last()
            && frame.kind == SvelteBlockKind::Element
        {
            scopes.pop();
        }
        return;
    }

    if trimmed.starts_with("<!") || trimmed.starts_with("<?") {
        return;
    }

    let parsed = parse_markup_tag(trimmed);
    if parsed.name.is_empty() {
        return;
    }

    let current = current_locals(scopes);
    if parsed.name.contains('.')
        || parsed
            .name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        merge_component_tag_usage(usage, &parsed.name, imported_bindings, &current, false);
    }

    let mut element_locals = Vec::new();
    for attr in &parsed.attrs {
        if let Some(binding) = directive_binding_name(&attr.name) {
            merge_expression_usage_allow_dollar_refs(usage, binding, imported_bindings, &current);
        }
        if let Some(local) = attr.name.strip_prefix("let:")
            && !local.is_empty()
        {
            element_locals.extend(extract_pattern_binding_names(local));
        }
        if let Some(expr) = shorthand_attribute_expression(&attr.name) {
            merge_expression_usage_allow_dollar_refs(usage, expr, imported_bindings, &current);
        }
        if let Some(value) = attr.value.as_deref() {
            merge_attribute_value_usage(usage, value, imported_bindings, &current);
        }
    }

    if !parsed.self_closing && !is_void_html_tag(&parsed.name) {
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Element,
            locals: element_locals,
        });
    }
}

#[derive(Debug)]
struct SvelteMarkupTag {
    name: String,
    attrs: Vec<SvelteMarkupAttr>,
    self_closing: bool,
}

#[derive(Debug)]
struct SvelteMarkupAttr {
    name: String,
    value: Option<String>,
}

fn parse_markup_tag(tag: &str) -> SvelteMarkupTag {
    let inner = tag.trim_start_matches('<').trim_end_matches('>').trim();
    let self_closing = inner.ends_with('/');
    let inner = inner.trim_end_matches('/').trim_end();

    let name_end = inner
        .char_indices()
        .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx))
        .unwrap_or(inner.len());
    let name = inner[..name_end].trim().to_string();

    let mut attrs = Vec::new();
    let mut index = name_end;
    while index < inner.len() {
        let remaining = &inner[index..];
        let trimmed = remaining.trim_start();
        index += remaining.len() - trimmed.len();
        if index >= inner.len() {
            break;
        }

        let name_end = inner[index..]
            .char_indices()
            .find_map(|(offset, ch)| (ch.is_whitespace() || ch == '=').then_some(index + offset))
            .unwrap_or(inner.len());
        let name = inner[index..name_end].trim();
        index = name_end;

        let remaining = &inner[index..];
        let trimmed = remaining.trim_start();
        index += remaining.len() - trimmed.len();

        let mut value = None;
        if inner.as_bytes().get(index) == Some(&b'=') {
            index += 1;
            let remaining = &inner[index..];
            let trimmed = remaining.trim_start();
            index += remaining.len() - trimmed.len();
            if let Some(quote) = inner.as_bytes().get(index).copied() {
                if quote == b'\'' || quote == b'"' {
                    let quote = quote as char;
                    index += 1;
                    let value_start = index;
                    while index < inner.len() && inner.as_bytes()[index] as char != quote {
                        index += 1;
                    }
                    value = Some(inner[value_start..index].to_string());
                    if index < inner.len() {
                        index += 1;
                    }
                } else if quote == b'{' {
                    let Some((expr, next_index)) = scan_curly_section(inner, index, 1, 1) else {
                        break;
                    };
                    value = Some(format!("{{{expr}}}"));
                    index = next_index;
                } else {
                    let value_end = inner[index..]
                        .char_indices()
                        .find_map(|(offset, ch)| ch.is_whitespace().then_some(index + offset))
                        .unwrap_or(inner.len());
                    value = Some(inner[index..value_end].to_string());
                    index = value_end;
                }
            }
        }

        if !name.is_empty() {
            attrs.push(SvelteMarkupAttr {
                name: name.to_string(),
                value,
            });
        }
    }

    SvelteMarkupTag {
        name,
        attrs,
        self_closing,
    }
}

fn directive_binding_name(attr_name: &str) -> Option<&str> {
    for prefix in ["use:", "animate:", "in:", "out:", "transition:"] {
        if let Some(rest) = attr_name.strip_prefix(prefix) {
            let binding = rest
                .split('|')
                .next()
                .map(str::trim)
                .filter(|name| !name.is_empty());
            if binding.is_some() {
                return binding;
            }
        }
    }
    None
}

fn shorthand_attribute_expression(attr_name: &str) -> Option<&str> {
    attr_name
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .map(str::trim)
        .filter(|expr| !expr.is_empty())
}

fn merge_attribute_value_usage(
    usage: &mut TemplateUsage,
    value: &str,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
) {
    let mut index = 0;
    let mut found_expression = false;
    let bytes = value.as_bytes();

    while index < bytes.len() {
        if bytes[index] == b'{' {
            let Some((expr, next_index)) = scan_curly_section(value, index, 1, 1) else {
                break;
            };
            merge_expression_usage_allow_dollar_refs(usage, expr, imported_bindings, locals);
            found_expression = true;
            index = next_index;
            continue;
        }
        index += 1;
    }

    if !found_expression && value.starts_with('{') && value.ends_with('}') && value.len() >= 2 {
        merge_expression_usage_allow_dollar_refs(
            usage,
            &value[1..value.len() - 1],
            imported_bindings,
            locals,
        );
    }
}

fn is_void_html_tag(tag_name: &str) -> bool {
    VOID_HTML_TAGS.contains(&tag_name)
}

fn pop_scope(scopes: &mut Vec<SvelteScopeFrame>, closing: &str) {
    let kind = match closing {
        "if" => Some(SvelteBlockKind::If),
        "each" => Some(SvelteBlockKind::Each),
        "await" => Some(SvelteBlockKind::Await),
        "key" => Some(SvelteBlockKind::Key),
        "snippet" => Some(SvelteBlockKind::Snippet),
        _ => None,
    };

    let Some(kind) = kind else {
        return;
    };

    if let Some(index) = scopes.iter().rposition(|frame| frame.kind == kind)
        && index > 0
    {
        scopes.truncate(index);
    }
}

fn current_locals(scopes: &[SvelteScopeFrame]) -> Vec<String> {
    scopes
        .iter()
        .flat_map(|frame| frame.locals.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::collect_template_usage;
    use rustc_hash::FxHashSet;

    fn imported(names: &[&str]) -> FxHashSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn plain_expression_marks_binding_used() {
        let usage = collect_template_usage(
            "<script>import { formatDate } from './utils';</script><p>{formatDate(value)}</p>",
            &imported(&["formatDate"]),
        );

        assert!(usage.used_bindings.contains("formatDate"));
    }

    #[test]
    fn each_alias_shadows_import_name() {
        let usage = collect_template_usage(
            "<script>import { item } from './utils';</script>{#each items as item}<p>{item}</p>{/each}",
            &imported(&["item"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn await_then_alias_shadows_import_name() {
        let usage = collect_template_usage(
            "<script>import { value } from './utils';</script>{#await promise}{:then value}<p>{value}</p>{/await}",
            &imported(&["value"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn namespace_member_accesses_are_retained() {
        let usage = collect_template_usage(
            "<script>import * as utils from './utils';</script><p>{utils.formatDate(value)}</p>",
            &imported(&["utils"]),
        );

        assert!(usage.used_bindings.contains("utils"));
        assert_eq!(usage.member_accesses.len(), 1);
        assert_eq!(usage.member_accesses[0].object, "utils");
        assert_eq!(usage.member_accesses[0].member, "formatDate");
    }

    #[test]
    fn styles_are_ignored() {
        let usage = collect_template_usage(
            "<style>.button { color: red; }</style><script>import { button } from './utils';</script>",
            &imported(&["button"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn component_tags_mark_imported_components_used() {
        let usage = collect_template_usage(
            "<script>import FancyButton from './FancyButton.svelte';</script><FancyButton />",
            &imported(&["FancyButton"]),
        );

        assert!(usage.used_bindings.contains("FancyButton"));
    }

    #[test]
    fn namespaced_component_tags_record_member_usage() {
        let usage = collect_template_usage(
            "<script>import * as Icons from './icons';</script><Icons.Alert />",
            &imported(&["Icons"]),
        );

        assert!(usage.used_bindings.contains("Icons"));
        assert_eq!(usage.member_accesses.len(), 1);
        assert_eq!(usage.member_accesses[0].object, "Icons");
        assert_eq!(usage.member_accesses[0].member, "Alert");
    }

    #[test]
    fn directive_names_mark_imported_actions_used() {
        let usage = collect_template_usage(
            "<script>import { tooltip } from './actions';</script><button use:tooltip>Hi</button>",
            &imported(&["tooltip"]),
        );

        assert!(usage.used_bindings.contains("tooltip"));
    }

    #[test]
    fn attribute_value_expressions_mark_imported_bindings_used() {
        let usage = collect_template_usage(
            "<script>import { isActive } from './state';</script><button class:active={isActive}>Hi</button>",
            &imported(&["isActive"]),
        );

        assert!(usage.used_bindings.contains("isActive"));
    }

    #[test]
    fn shorthand_attributes_mark_imported_bindings_used() {
        let usage = collect_template_usage(
            "<script>import { page } from './stores';</script><Component {page} />",
            &imported(&["page"]),
        );

        assert!(usage.used_bindings.contains("page"));
    }

    #[test]
    fn dollar_store_refs_mark_imported_store_used() {
        let usage = collect_template_usage(
            "<script>import { page } from './stores';</script><p>{$page.url.pathname}</p>",
            &imported(&["page"]),
        );

        assert!(usage.used_bindings.contains("page"));
    }

    #[test]
    fn let_directives_shadow_imported_names() {
        let usage = collect_template_usage(
            "<script>import { item } from './utils';</script><Slot let:item><p>{item}</p></Slot>",
            &imported(&["item"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn local_let_bindings_shadow_imported_component_tags() {
        let usage = collect_template_usage(
            "<script>import Item from './Item.svelte';</script><Slot let:Item><Item /></Slot>",
            &imported(&["Item"]),
        );

        assert!(usage.is_empty());
    }
}
