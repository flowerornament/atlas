//! Consumer selection over shared facts; selection never changes source classification.
use crate::{
    model::SourceFile,
    source::{Item, Source, Visibility},
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
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
pub enum TestSelection {
    All,
    Production,
    OutsideTestModules,
}
impl TestSelection {
    pub const fn admits_file(self, test_only: bool) -> bool {
        matches!(self, Self::All) || !test_only
    }
    /// One test predicate for lexical items, imports and AST projections.
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
        let selected =
            self.tests.admits_file(file.test_only) && self.tests.admits_line(source, item.line);
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
    pub fn admits(self, span: proc_macro2::Span) -> bool {
        match self {
            Self::All => true,
            Self::Selected { source, tests } => tests.admits_line(source, span.start().line),
        }
    }
}
