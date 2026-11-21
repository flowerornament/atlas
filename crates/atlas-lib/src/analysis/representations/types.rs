//! Source facts retain spelling and shape rather than guessing ownership semantics.
use crate::workflow::{Call, Site};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Type {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub qualified: String,
    pub file: String,
    pub line: usize,
    pub fields: Vec<(String, String)>,
    /// Written RHS of a named alias; never expanded or treated as retained fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// As [`Function::test_only`].
    #[serde(default)]
    pub test_only: bool,
}
impl Type {
    /// Declaration label preserves the distinction between an alias and a nominal type.
    pub fn declaration(&self) -> String {
        self.alias.as_ref().map_or_else(
            || self.name.clone(),
            |rhs| format!("type {} = {rhs}", self.name),
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Function {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub qualified: String,
    pub file: String,
    pub line: usize,
    pub signature: String,
    pub public: bool,
    pub trait_impl: bool,
    /// In a test-only file, a test module, or behind a test gate; only a
    /// selection that includes tests has such functions.
    #[serde(default)]
    pub test_only: bool,
    /// Code lines in the body and the deepest nesting, as the syntax pass
    /// measured them ([`crate::source::FunctionMeasure`]): the numbers the
    /// concentration law reads.
    #[serde(default)]
    pub lines: usize,
    #[serde(default)]
    pub nesting: usize,
}
impl Function {
    /// Unqualified impl-target type named by this written method name, not a retention claim.
    /// Free functions and unsupported owner syntax have no such head.
    pub fn owner_name(&self) -> Option<String> {
        let (owner, _) = self.name.rsplit_once("::")?;
        let syn::Type::Path(path) = syn::parse_str::<syn::Type>(owner).ok()? else {
            return None;
        };
        Some(path.path.segments.last()?.ident.to_string())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Role {
    Input,
    Output,
    Field,
    /// A named type mentioned in an alias RHS, not a retained field.
    AliasTarget,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Reference {
    pub from: String,
    pub to: String,
    pub role: Role,
    pub slot: String,
    /// Entire written type: `Arc<Graph>` is not represented as owning Graph.
    pub expression: String,
    /// Start line of the written type expression, before container traversal.
    #[serde(default)]
    pub line: usize,
    pub borrow: bool,
    pub wrappers: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Constructor {
    pub from: String,
    pub to: String,
    pub line: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ResolvedCall {
    pub from: String,
    pub to: String,
    pub site: Site,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UnknownCall {
    pub from: String,
    pub call: Call,
    /// The readings of `reasons`, joined with `; `.
    pub reason: String,
    /// Why it is unresolved, one per distinct reason, with stable kinds.
    #[serde(default)]
    pub reasons: Vec<crate::analysis::index::Summary>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UnknownType {
    pub from: String,
    pub spelling: String,
    pub role: Role,
    pub reason: String,
    /// The parameter, return or field the spelling occupies, as for a
    /// [`Reference`].
    #[serde(default)]
    pub slot: String,
    /// Where the spelling is written (1-based), so each mention is its own row.
    #[serde(default)]
    pub line: usize,
    #[serde(default)]
    pub column: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Facts {
    pub schema: u32,
    pub types: Vec<Type>,
    pub functions: Vec<Function>,
    pub references: Vec<Reference>,
    pub constructors: Vec<Constructor>,
    #[serde(default)]
    pub constants: Vec<Constant>,
    /// Explicit zero-argument Type::default() calls; no trait dispatch or freshness claim.
    #[serde(default)]
    pub default_calls: Vec<Constructor>,
    pub calls: Vec<ResolvedCall>,
    /// Includes external and primitive spellings; absence is never inferred from these.
    pub unknown_types: Vec<UnknownType>,
    pub unknown_calls: Vec<UnknownCall>,
    pub fingerprints: Vec<(String, String)>,
    pub limitations: Vec<String>,
}

/// A written constant, not a compile-time evaluated value.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Constant {
    pub name: String,
    pub module: Vec<String>,
    pub site: Site,
    pub value: Literal,
    /// As [`Function::test_only`].
    #[serde(default)]
    pub test_only: bool,
}
impl Constant {
    /// Source-qualified identity within one snapshot; line numbers remain witnesses.
    pub fn id(&self) -> String {
        std::iter::once(self.site.path.as_str())
            .chain(self.module.iter().map(String::as_str))
            .chain(std::iter::once(self.name.as_str()))
            .collect::<Vec<_>>()
            .join("::")
    }
}
/// Supported literal syntax. Opaque expressions are preserved, never executed.
/// New syntax forms may be added; match with a wildcard arm.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Literal {
    String(String),
    Array(Vec<Self>),
    Tuple(Vec<Self>),
    Path(Vec<String>),
    /// A call whose callee is a plain path, such as `Some(Part::Host)` or
    /// `Wrapper(1, X)`. This records syntax only: a tuple struct, an enum
    /// variant and a function call look the same, and nothing is evaluated.
    Call {
        path: Vec<String>,
        args: Vec<Self>,
    },
    Opaque(String),
}
impl Literal {
    pub fn string(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub fn array(&self) -> Option<&[Self]> {
        if let Self::Array(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn tuple(&self) -> Option<&[Self]> {
        if let Self::Tuple(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn path(&self) -> Option<&[String]> {
        if let Self::Path(v) = self {
            Some(v)
        } else {
            None
        }
    }
    /// The callee path and arguments of a [`Literal::Call`].
    pub fn call(&self) -> Option<(&[String], &[Self])> {
        if let Self::Call { path, args } = self {
            Some((path, args))
        } else {
            None
        }
    }
}

/// The file an observation ID (`path:line:column`) was written in.
fn file_of(id: &str) -> &str {
    let mut parts = id.rsplitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_), Some(_), Some(path)) => path,
        _ => id,
    }
}

impl Facts {
    /// The observations incident to the files `in_scope` admits: every call,
    /// reference and construction with an end in them, the unresolved calls
    /// and types written in them, and the functions and types at both ends.
    /// Nothing is resolved anew; observations elsewhere are left out, not
    /// changed, and the limitations say so.
    #[must_use]
    pub fn incident(&self, scope: &str, in_scope: impl Fn(&str) -> bool) -> Self {
        let touches = |from: &str, to: &str| in_scope(file_of(from)) || in_scope(file_of(to));
        let calls = kept(&self.calls, |c| touches(&c.from, &c.to));
        let references = kept(&self.references, |r| touches(&r.from, &r.to));
        let constructors = kept(&self.constructors, |c| touches(&c.from, &c.to));
        let default_calls = kept(&self.default_calls, |c| touches(&c.from, &c.to));
        let mut ends: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for (from, to) in calls
            .iter()
            .map(|c| (&c.from, &c.to))
            .chain(references.iter().map(|r| (&r.from, &r.to)))
            .chain(constructors.iter().map(|c| (&c.from, &c.to)))
            .chain(default_calls.iter().map(|c| (&c.from, &c.to)))
        {
            ends.insert(from);
            ends.insert(to);
        }
        let functions = kept(&self.functions, |f| {
            in_scope(&f.file) || ends.contains(f.id.as_str())
        });
        let types = kept(&self.types, |t| {
            in_scope(&t.file) || ends.contains(t.id.as_str())
        });
        let files: std::collections::BTreeSet<String> = functions
            .iter()
            .map(|f| f.file.clone())
            .chain(types.iter().map(|t| t.file.clone()))
            .collect();
        let mut limitations = self.limitations.clone();
        limitations.push(format!(
            "Only observations incident to `{scope}` are listed; the analysis read the whole workspace."
        ));
        Self {
            schema: self.schema,
            types,
            functions,
            references,
            constructors,
            constants: kept(&self.constants, |c| in_scope(&c.site.path)),
            default_calls,
            calls,
            unknown_types: kept(&self.unknown_types, |u| in_scope(file_of(&u.from))),
            unknown_calls: kept(&self.unknown_calls, |u| in_scope(file_of(&u.from))),
            fingerprints: kept(&self.fingerprints, |(path, _)| {
                in_scope(path) || files.contains(path)
            }),
            limitations,
        }
    }
}

/// The observations of `list` that `keep` admits, unchanged.
fn kept<T: Clone>(list: &[T], keep: impl Fn(&T) -> bool) -> Vec<T> {
    list.iter().filter(|item| keep(item)).cloned().collect()
}
