//! Schema v1 of the flat relations: every relation, column, resolution basis
//! and unknowns companion, as data. This is the contract `facts --schema`
//! prints and `docs/facts.md` describes. Within one schema version a change
//! may add relations or append columns, never rename, remove or reorder them.
use serde::Serialize;

/// The schema version exports carry. It changes only when a relation or
/// column is renamed, removed or changes meaning.
pub const VERSION: u32 = 1;

/// The stage whose observations a relation holds. Families are never merged:
/// a lexical mention and a qualified reference are different evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Family {
    /// Names written in the source, matched by spelling (the `dump` graph).
    Lexical,
    /// Names followed through `use` statements and modules (the analysis
    /// behind `dependencies` and `types`).
    Qualified,
    /// A compiler front end's reading, from a SCIP index (rust-analyzer's):
    /// present only when a command is given one.
    Semantic,
}
impl Family {
    /// Every family, in export order.
    pub const ALL: [Self; 3] = [Self::Lexical, Self::Qualified, Self::Semantic];
    /// The name used on the command line and in exports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Qualified => "qualified",
            Self::Semantic => "semantic",
        }
    }
}

/// A column's value type. There are no nulls: a column that can be empty
/// says in its description which value means "none" (an empty string or 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Kind {
    Text,
    Integer,
    Boolean,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Column {
    pub name: &'static str,
    pub kind: Kind,
    pub description: &'static str,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct Relation {
    pub name: &'static str,
    pub family: Family,
    pub description: &'static str,
    pub columns: &'static [Column],
    /// The column naming how a row's target was selected (`unique` or
    /// `candidate`). A query that reads the relation must keep it or require
    /// one value; dropping it turns a candidate into a claim.
    pub basis: Option<&'static str>,
    /// The relation holding the observations this one could not resolve.
    /// The absence of a row here is not evidence of absence while the
    /// companion has rows that could match.
    pub unknowns: Option<&'static str>,
    /// Limits of the observation that the columns cannot show.
    pub notes: &'static [&'static str],
}
impl Relation {
    /// The position of `column`, if the relation has it.
    #[must_use]
    pub fn position(&self, column: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == column)
    }
}

const fn text(name: &'static str, description: &'static str) -> Column {
    Column {
        name,
        kind: Kind::Text,
        description,
    }
}
const fn integer(name: &'static str, description: &'static str) -> Column {
    Column {
        name,
        kind: Kind::Integer,
        description,
    }
}
const fn boolean(name: &'static str, description: &'static str) -> Column {
    Column {
        name,
        kind: Kind::Boolean,
        description,
    }
}

const ID_IN_EXPORT: &str =
    "Witness id, unique within this relation in one export; not stable between exports.";

/// Every relation of schema v1, lexical first, in export order.
pub const RELATIONS: &[Relation] = &[
    Relation {
        name: "file",
        family: Family::Lexical,
        description: "One package's observation of a source file. A path that two packages read has two rows; Atlas never picks an owner.",
        columns: &[
            text("path", "Path relative to the workspace root."),
            text("package", "The Cargo package that reads the file."),
            boolean(
                "test_only",
                "The package reads it only in a test build (a test target, or under a test module).",
            ),
            text(
                "fingerprint",
                "FNV-1a 64-bit content fingerprint; a change detector, not a security digest.",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[
            "Includes files no module declaration reaches (disconnected source): they have no mount row.",
        ],
    },
    Relation {
        name: "mount",
        family: Family::Lexical,
        description: "A module path at which a package's Cargo target reaches a file through `mod` declarations.",
        columns: &[
            text("path", "The file, as in `file.path`."),
            text("package", "The package, as in `file.package`."),
            text("root", "The Cargo target root the path starts from."),
            text(
                "module_path",
                "Modules from the target root, joined with `::`; empty for the root file itself.",
            ),
            text(
                "target",
                "The Cargo target, when several share the root; empty when one target starts there (most rows), since the root then names it.",
            ),
            boolean(
                "test_only",
                "The target or a module on the path requires a test build.",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &["A file can have several mounts (several targets, or `#[path]`), or none."],
    },
    Relation {
        name: "declaration",
        family: Family::Lexical,
        description: "A declaration site: an item Atlas read at a file and line. Not a resolved Rust symbol.",
        columns: &[
            text(
                "id",
                "`path:line:name`. A file two packages read gives the same id in each package.",
            ),
            text(
                "package",
                "The package whose observation of the file this is.",
            ),
            text("path", "The file."),
            text(
                "name",
                "The declared name as written (for an impl, its written head).",
            ),
            text(
                "kind",
                "Fn, Struct, Enum, Union, Trait, TypeAlias, Mod, MacroRules, PubUse, Const, Static, Impl, Use, MacroCall or Extern; an unknown tag is kept verbatim.",
            ),
            text(
                "visibility",
                "Written visibility: Pub, PubCrate, PubIn, PubSuper or Private. Not Rust's effective visibility.",
            ),
            integer("line", "First line of the item, 1-based."),
            integer("end_line", "Last line of the item."),
            text(
                "inline_modules",
                "Enclosing inline `mod` blocks in the file, outermost first, joined with `::`; empty at file level.",
            ),
            boolean(
                "test_only",
                "Inside a test module, or in a file the package reads only for tests.",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[
            "The key is (id, package). Count declarations per package observation, as `census` does.",
        ],
    },
    Relation {
        name: "import",
        family: Family::Lexical,
        description: "A `use` declaration as written, per package observation of its file.",
        columns: &[
            text("path", "The file."),
            text("package", "The package."),
            text(
                "written",
                "The `use` tree as written, without resolving it.",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[],
    },
    Relation {
        name: "mention",
        family: Family::Lexical,
        description: "A type name written in a declaration's signature or body, matched by spelling.",
        columns: &[
            text("id", "Mention id, unique within one export."),
            text(
                "source",
                "The declaration whose text holds the mention (`declaration.id`).",
            ),
            text("spelling", "The identifier as written."),
            text(
                "evidence",
                "parameter-mention, return-mention or type-body-mention.",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &["Approximate: a spelling match, not compiler name resolution."],
    },
    Relation {
        name: "mention_target",
        family: Family::Lexical,
        description: "A declaration a mention's spelling can name. An ambiguous mention has one row per candidate.",
        columns: &[
            text("mention", "The mention (`mention.id`)."),
            text("target", "A candidate declaration (`declaration.id`)."),
            text(
                "basis",
                "`unique` when the spelling names one declaration; `candidate` when it names several and this is one of them.",
            ),
        ],
        basis: Some("basis"),
        unknowns: Some("mention_unresolved"),
        notes: &[
            "A `unique` basis is a unique spelling match, not proof that Rust resolves the name there.",
        ],
    },
    Relation {
        name: "mention_unresolved",
        family: Family::Lexical,
        description: "A mention whose spelling Atlas could not match to a workspace declaration.",
        columns: &[
            text("mention", "The mention (`mention.id`)."),
            text("reason", "Why it is unresolved, as Atlas reports it."),
        ],
        basis: None,
        unknowns: None,
        notes: &[],
    },
    Relation {
        name: "diagnostic",
        family: Family::Lexical,
        description: "An observation Atlas could not complete while reading the source.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text("path", "The file."),
            integer(
                "line",
                "The line, or 0 when the diagnostic is about the whole file.",
            ),
            text(
                "package",
                "The package whose observation it is, or empty for the physical file.",
            ),
            text(
                "message",
                "The diagnostic as written in Atlas's other output; its wording may change.",
            ),
            text(
                "code",
                "Its kind, a stable code such as `mount-missing` (see the code list).",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[],
    },
    Relation {
        name: "qualified_file",
        family: Family::Qualified,
        description: "A file the qualified analysis read.",
        columns: &[
            text("path", "The file."),
            text("fingerprint", "Content fingerprint of the bytes analyzed."),
        ],
        basis: None,
        unknowns: None,
        notes: &[
            "Production source by default: test targets, test modules and test-gated items are read only with --with-tests, and each row from them is marked test_only.",
        ],
    },
    Relation {
        name: "function",
        family: Family::Qualified,
        description: "A function body the qualified analysis selected.",
        columns: &[
            text("id", "`path:line:column` of the item."),
            text("path", "The file."),
            integer("line", "First line of the item."),
            text(
                "name",
                "The name as written, with the impl type for a method (`Type::name`).",
            ),
            text(
                "qualified",
                "The name with the inline `mod` blocks that enclose it, and the owner type for a method (`Snapshot::graph`); the file's own module path is `mount.module_path`, one per Cargo target that reaches the file.",
            ),
            text("signature", "The signature as written."),
            boolean(
                "public",
                "Written `pub` with no restriction. `pub(crate)`, `pub(super)`, `pub(in path)` and private are false, and so is a trait impl's method, which has the trait's visibility.",
            ),
            boolean("trait_impl", "Declared in a trait impl."),
            text(
                "declaration",
                "The lexical `declaration.id` at the same file, line and name; empty if none matched.",
            ),
            boolean(
                "test_only",
                "In a test-only file, a test module, or behind a test gate. Always false unless the capture includes tests (`--with-tests`).",
            ),
            integer(
                "lines",
                "Code lines in the body, as Clippy's too_many_lines counts them: lines holding code outside comments, the outer braces left out. The concentration law's measure.",
            ),
            integer(
                "nesting",
                "The deepest block level inside the function, counted from the file's root as Clippy's excessive_nesting counts it. Blocks inside macro arguments are not counted.",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[],
    },
    Relation {
        name: "type",
        family: Family::Qualified,
        description: "A workspace struct, enum or type alias.",
        columns: &[
            text("id", "`path:line:column` of the item."),
            text("path", "The file."),
            integer("line", "First line of the item."),
            text("name", "The name as written."),
            text(
                "qualified",
                "The name with the inline `mod` blocks that enclose it; the file's own module path is `mount.module_path`, one per Cargo target that reaches the file.",
            ),
            text(
                "alias",
                "For a type alias, its right-hand side as written; empty for a struct or enum.",
            ),
            text(
                "declaration",
                "The lexical `declaration.id` at the same file, line and name; empty if none matched.",
            ),
            boolean(
                "test_only",
                "In a test-only file, a test module, or behind a test gate. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &["An alias is never expanded."],
    },
    Relation {
        name: "field",
        family: Family::Qualified,
        description: "A field of a workspace struct or enum, with its type as written.",
        columns: &[
            text("type", "The type (`type.id`)."),
            integer("position", "Position among the type's fields, from 0."),
            text("name", "The field name, or its position for a tuple field."),
            text("written", "The field's type as written."),
        ],
        basis: None,
        unknowns: None,
        notes: &[],
    },
    Relation {
        name: "call",
        family: Family::Qualified,
        description: "A call whose target Atlas resolved to exactly one workspace function.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text("from", "The calling function (`function.id`)."),
            text("to", "The called function (`function.id`)."),
            text("path", "File of the call site."),
            integer("line", "Line of the call site."),
            integer("column", "Column of the call site."),
            boolean(
                "test_only",
                "The `from` item is `test_only`: the observation is in test code. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: Some("unknown_call"),
        notes: &[
            "Only unique resolutions: an ambiguous or unresolved call is an `unknown_call` row, never a `call` row.",
            "Calls inside macro arguments are not observed; a closure's or `async` block's calls are its enclosing function's.",
        ],
    },
    Relation {
        name: "unknown_call",
        family: Family::Qualified,
        description: "A call Atlas could not resolve to exactly one workspace function: external, ambiguous or unsupported.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text("from", "The calling function (`function.id`)."),
            text("spelling", "The callee as written."),
            text(
                "receiver",
                "The method receiver as written, or empty for a path call.",
            ),
            text("reason", "Why the call is unresolved, as Atlas reports it."),
            text("path", "File of the call site."),
            integer("line", "Line of the call site."),
            integer("column", "Column of the call site."),
            text(
                "kind",
                "Each reason's stable kind, such as `unresolved-path`, joined with `; ` like `reason`.",
            ),
            text(
                "subject",
                "Each reason's path, name, receiver, type or module, or empty, joined the same way.",
            ),
            boolean(
                "test_only",
                "The `from` item is `test_only`: the observation is in test code. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[
            "Includes calls into other crates and the standard library; an unknown call is not a missing call.",
        ],
    },
    Relation {
        name: "reference",
        family: Family::Qualified,
        description: "A signature or field type that names exactly one workspace type.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text(
                "from",
                "The function or type whose signature or field it is.",
            ),
            text("to", "The named type (`type.id`)."),
            text("role", "input, output, field or alias-target."),
            text("slot", "The parameter, return or field it occupies."),
            text(
                "expression",
                "The entire written type, such as `Arc<Graph>`.",
            ),
            integer("line", "Line of the written type."),
            boolean("borrow", "The written type is a reference."),
            boolean(
                "test_only",
                "The `from` item is `test_only`: the observation is in test code. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: Some("unknown_type"),
        notes: &["`Arc<Graph>` is a reference to Graph, not a claim that the function owns one."],
    },
    Relation {
        name: "unknown_type",
        family: Family::Qualified,
        description: "A type spelling in a signature or field that Atlas did not resolve to one workspace type.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text(
                "from",
                "The function or type whose signature or field it is.",
            ),
            text("spelling", "The type as written."),
            text("role", "input, output, field or alias-target."),
            text("reason", "Why it is unresolved, as Atlas reports it."),
            text("slot", "The parameter, return or field it occupies."),
            integer("line", "Line of the spelling."),
            integer("column", "Column of the spelling, 1-based."),
            boolean(
                "test_only",
                "The `from` item is `test_only`: the observation is in test code. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[
            "Includes external and primitive types.",
            "Each mention is its own row: a generic argument such as the `u32` in `Vec<u32>` is a row of its own beside `Vec<u32>`.",
        ],
    },
    Relation {
        name: "construction",
        family: Family::Qualified,
        description: "A struct literal or enum variant of a workspace type built in a function body.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text("from", "The function (`function.id`)."),
            text("to", "The type built (`type.id`)."),
            integer("line", "Line of the construction."),
            boolean(
                "test_only",
                "The `from` item is `test_only`: the observation is in test code. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[],
    },
    Relation {
        name: "default_call",
        family: Family::Qualified,
        description: "An explicit zero-argument `Type::default()` call for a workspace type.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text("from", "The function (`function.id`)."),
            text("to", "The type (`type.id`)."),
            integer("line", "Line of the call."),
            boolean(
                "test_only",
                "The `from` item is `test_only`: the observation is in test code. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &["No trait dispatch is followed."],
    },
    Relation {
        name: "constant",
        family: Family::Qualified,
        description: "A written `const` item and its value as syntax; nothing is evaluated.",
        columns: &[
            text("id", "`path::modules::NAME`, unique within one export."),
            text("path", "The file."),
            integer("line", "Line of the item."),
            text("name", "The constant's name."),
            text(
                "inline_modules",
                "Enclosing inline `mod` blocks, joined with `::`; empty at file level.",
            ),
            text(
                "value",
                "The value as JSON: `{\"kind\": ..., \"value\": ...}` with kind string, array, tuple, path, call or opaque.",
            ),
            boolean(
                "test_only",
                "In a test-only file, a test module, or behind a test gate. Always false unless the capture includes tests (`--with-tests`).",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[],
    },
    Relation {
        name: "semantic_call",
        family: Family::Semantic,
        description: "A recorded call that the SCIP index (a compiler front end's reading) names a workspace function for.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text(
                "from",
                "The calling function (`function.id`), or empty when no captured function is at the caller's site.",
            ),
            text(
                "to",
                "The function the index defines the callee at (`function.id`): the captured function whose file and line are the definition's.",
            ),
            text("spelling", "The callee as written, as in `unknown_call`."),
            text(
                "path",
                "File of the call site, as in `call` and `unknown_call`, so the readings join on site and spelling (chained calls share a site).",
            ),
            integer("line", "Line of the call site."),
            integer("column", "Column of the call site."),
            text(
                "agreement",
                "How the qualified reading compares: `agrees` (the same function), `extends` (qualified named no workspace function), `disagrees` (they differ: a finding, reported, never resolved).",
            ),
        ],
        basis: None,
        unknowns: Some("semantic_unknown"),
        notes: &[
            "One build configuration: the indexer's host target and default features; code inactive there has no reading.",
            "Read only from an index the command is given (--scip FILE) or makes (--semantic); a semantic row is not a `call` row and is never merged into one.",
            "A row requires the index's symbol to name what is written at the site; a stale index reads as `mismatch` in `semantic_unknown`.",
        ],
    },
    Relation {
        name: "semantic_unknown",
        family: Family::Semantic,
        description: "A recorded call the SCIP index names no captured workspace function for.",
        columns: &[
            text("id", ID_IN_EXPORT),
            text("from", "The calling function (`function.id`), or empty."),
            text("spelling", "The callee as written."),
            text("path", "File of the call site."),
            integer("line", "Line of the call site."),
            integer("column", "Column of the call site."),
            text(
                "reading",
                "What the index says: `trait-method` (dispatch open), `external` (another package), `no-definition` (a workspace symbol defined nowhere: macro- or derive-generated, or inactive), `local` (a closure or function value), `ambiguous`, `mismatch` (the index names another symbol: made from other source), `not-indexed`, or `definition` (a definition with no captured function at its site).",
            ),
            text(
                "detail",
                "The trait declaration's site, the external package and item, the symbol, or the reason.",
            ),
        ],
        basis: None,
        unknowns: None,
        notes: &[
            "A semantic unknown is not a missing call; the qualified `call` and `unknown_call` rows at the same site still stand.",
        ],
    },
];

/// The schema entry for `name`.
#[must_use]
pub fn relation(name: &str) -> Option<&'static Relation> {
    RELATIONS.iter().find(|r| r.name == name)
}
