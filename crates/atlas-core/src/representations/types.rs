//! Source facts retain spelling and shape rather than guessing ownership semantics.
use crate::workflow::{Call, Site};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Type {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub qualified: String,
    pub file: String,
    pub line: usize,
    pub fields: Vec<(String, String)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
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
pub enum Role {
    Input,
    Output,
    Field,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
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
pub struct Constructor {
    pub from: String,
    pub to: String,
    pub line: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedCall {
    pub from: String,
    pub to: String,
    pub site: Site,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnknownCall {
    pub from: String,
    pub call: Call,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnknownType {
    pub from: String,
    pub spelling: String,
    pub role: Role,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
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
pub struct Constant {
    pub name: String,
    pub module: Vec<String>,
    pub site: Site,
    pub value: Literal,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Literal {
    String(String),
    Array(Vec<Self>),
    Tuple(Vec<Self>),
    Path(Vec<String>),
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
}
