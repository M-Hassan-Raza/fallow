mod astro;
mod css;
mod graphql;
mod js_ts;
mod mdx;
mod regex_compile;
mod sfc;

use std::path::Path;

use fallow_types::discover::FileId;
use fallow_types::extract::ModuleInfo;

use crate::parse::parse_source_to_module;

/// Shared test helper: parse TypeScript source and return `ModuleInfo`.
pub fn parse_ts(source: &str) -> ModuleInfo {
    parse_source_to_module(FileId(0), Path::new("test.ts"), source, 0, false)
}

/// Shared test helper: parse TypeScript source with complexity metrics.
pub fn parse_ts_with_complexity(source: &str) -> ModuleInfo {
    parse_source_to_module(FileId(0), Path::new("test.ts"), source, 0, true)
}

/// Shared test helper: parse TSX source and return `ModuleInfo`.
pub fn parse_tsx(source: &str) -> ModuleInfo {
    parse_source_to_module(FileId(0), Path::new("test.tsx"), source, 0, false)
}

#[test]
fn parses_glimmer_typescript_as_typescript() {
    let info = parse_source_to_module(
        FileId(0),
        Path::new("component.gts"),
        "import type Service from './service';\nexport type ServiceRef = Service;\n",
        0,
        false,
    );

    assert_eq!(info.imports.len(), 1);
    assert_eq!(info.imports[0].source, "./service");
    assert!(info.imports[0].is_type_only);
    assert!(
        info.exports
            .iter()
            .any(|export| export.name.matches_str("ServiceRef"))
    );
}
