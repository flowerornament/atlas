//! Consumer selection over shared facts; selection never changes source classification.
use crate::{
    model::SourceFile,
    source::{Item, Source, Visibility},
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum VisibilitySelection {
    All,
    Public,
    RestrictedOrPublic,
}
impl VisibilitySelection {
    pub const fn admits(self, visibility: Visibility) -> bool {
        match self {
            Self::All => true,
            Self::Public => matches!(visibility, Visibility::Pub),
            Self::RestrictedOrPublic => !matches!(visibility, Visibility::Private),
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Public => "pub",
            Self::RestrictedOrPublic => "pub-ish",
        }
    }
}
impl std::str::FromStr for VisibilitySelection {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "all" => Ok(Self::All),
            "pub" | "public" => Ok(Self::Public),
            "pub-ish" => Ok(Self::RestrictedOrPublic),
            _ => Err(format!(
                "unknown population `{value}`; choose all, pub or pub-ish"
            )),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TestSelection {
    All,
    Production,
    OutsideTestModules,
}
impl TestSelection {
    pub const fn admits_file(self, test_only: bool) -> bool {
        matches!(self, Self::All) || !test_only
    }
    /// Exact syntax-site selection, shared by item, import and AST projections.
    pub fn admits_offset(self, source: &Source, offset: usize) -> bool {
        match self {
            Self::All => true,
            Self::OutsideTestModules => !source.module_test_at(offset),
            Self::Production => !source.module_test_at(offset) && !source.gated_at(offset),
        }
    }
    /// Native line accounting, distinct from exact syntax-site selection.
    pub fn admits_line(self, source: &Source, line: usize) -> bool {
        match self {
            Self::All => true,
            Self::OutsideTestModules => !source.test_regions().covers(line),
            Self::Production => {
                !source.test_regions().covers(line) && !source.individually_test_gated(line)
            }
        }
    }
}
/// File membership is independent of declaration visibility and test evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum FileSelection {
    /// Every discovered package file, including Cargo roots outside src.
    Discovered,
    /// Only the package's src inventory, as used by native source budgets.
    SourceTree,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Population {
    pub visibility: VisibilitySelection,
    pub tests: TestSelection,
}
impl Population {
    /// File, module and directly gated item evidence remain independently selectable.
    pub fn admits(self, file: &SourceFile, source: &Source, item: &Item) -> bool {
        let visible = self.visibility.admits(item.visibility);
        let selected = self.tests.admits_file(file.test_only)
            && self.tests.admits_offset(source, item.syntax_span.start);
        visible && selected
    }
}

/// Test selection over one file's retained lexical evidence, shared by AST readers.
#[derive(Clone, Copy)]
pub(crate) enum SyntaxSelection<'a> {
    All,
    Selected {
        source: &'a Source,
        tests: TestSelection,
    },
}
impl SyntaxSelection<'_> {
    /// Whether the site is test code: inside a test module or test-gated.
    /// A whole test-only file is the caller's to add; `All` knows no source.
    pub fn test_site(self, span: proc_macro2::Span) -> bool {
        match self {
            Self::All => false,
            Self::Selected { source, .. } => {
                !TestSelection::Production.admits_offset(source, span.byte_range().start)
            }
        }
    }
    pub fn admits(self, span: proc_macro2::Span) -> bool {
        match self {
            Self::All => true,
            Self::Selected { source, tests } => {
                tests.admits_offset(source, span.byte_range().start)
            }
        }
    }
}

/// Native line/mount policies cannot claim a complete reading at these sites.
/// Qualified byte-site analysis keeps its separate, more precise contract.
pub(crate) fn native_limitation(source: &Source) -> Option<(usize, &'static str)> {
    for item in source.outline() {
        let by_line = TestSelection::Production.admits_line(source, item.line);
        let by_site = TestSelection::Production.admits_offset(source, item.syntax_span.start);
        if by_line != by_site {
            return Some((
                item.line,
                "mixed test/production declarations on one native accounting line; place them on separate lines",
            ));
        }
        // A file module inside inline modules is mounted; one inside any
        // other item (a function body, say) is not.
        if by_site
            && item.kind == super::ItemKind::Mod
            && !item.top_level
            && item.declaration_block.trim_end().ends_with(';')
            && !source
                .file_module_declarations()
                .iter()
                .any(|module| module.item == item.syntax_span.start)
        {
            return Some((
                item.line,
                "nested file module has no supported native mount interpretation",
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limitation(text: &str) -> Option<(usize, &'static str)> {
        let captured =
            crate::source::capture(std::path::Path::new("."), "src/lib.rs", text.to_owned());
        native_limitation(captured.facts().expect("parses"))
    }

    /// A gated block in a production function owns its lines: the `use` in
    /// it is test code by line and by site alike.
    #[test]
    fn a_test_gated_block_in_a_production_function_is_not_mixed() {
        let shaped = "static SEALS: std::sync::OnceLock<u8> = std::sync::OnceLock::new();\npub fn seal() {\n    #[cfg(test)]\n    {\n        use std::fmt::Write as _;\n        let _ = SEALS.set(1);\n    }\n    let _ = 1;\n}\n";
        assert_eq!(limitation(shaped), None);
        let gated_let =
            "pub fn f() {\n    #[cfg(test)]\n    let _a = { use std::fmt::Write as _; 1 };\n}\n";
        assert_eq!(limitation(gated_let), None);
    }

    /// A gated statement that shares a line with production code leaves that
    /// line mixed.
    #[test]
    fn a_gated_statement_sharing_a_line_with_production_code_is_mixed() {
        let shared =
            "pub fn f() {\n    let _p = 1; #[cfg(test)] { use std::fmt::Write as _; }\n}\n";
        assert_eq!(limitation(shared).map(|(line, _)| line), Some(2));
    }
}
