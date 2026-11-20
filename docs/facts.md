# Facts: Atlas's observations as tables

`cargo atlas facts` exports what Atlas read as flat relations (tables with
named columns), so you can answer questions no command asks directly, with
jq, a script or a database. Every relation, column and companion described
below is also printed, without reading any source, by:

```sh
cargo atlas facts --schema             # readable: relation, column, kind, meaning
cargo atlas facts --schema --as json   # the same, as data
```

The export adds no evidence of its own. It flattens the same observations
that `dump` (the lexical graph) and `dependencies`, `types` and `overview`
(the qualified analysis) report in their own shapes.

## Exporting

```sh
cargo atlas facts                                   # one JSON document on stdout
cargo atlas facts --as jsonl --out facts.jsonl      # one object per row
cargo atlas facts --families lexical                # skip the slower qualified analysis
```

- **JSON** (`--as json`, the default) is one document: `schema`,
  `families`, `limitations`, and `relations`, an object with one entry per
  relation holding its `family`, `columns`, `basis`, `unknowns` and `rows`.
  Each row is an array in column order.
- **JSON Lines** (`--as jsonl`) starts with a header object (`schema`,
  `families`, `limitations`); every later line is one row, an object with a
  `relation` key and one key per column.
- `--out FILE` writes the export to a file. Atlas refuses a destination that
  is one of the project's sources or manifests.

There are no nulls. A column that can be empty says which value means
"none": an empty string, or 0 for a line.

## Two families, kept apart

- **Lexical** relations hold names as written, matched by spelling: files,
  their module paths, declarations, `use` statements, type mentions and
  diagnostics. Identity is `path:line:name`.
- **Qualified** relations hold what the analysis resolved by following `use`
  statements and modules: functions, types, fields, calls, type references,
  constructions and constants, and everything it could not resolve.
  Identity is `path:line:column`. This family covers production source only;
  `limitations` in the export says what else it leaves out.

Rows of different families are different evidence: a lexical mention of
`Token` is not a qualified reference to it. To join them, use the
`declaration` column of `function` and `type`, which names the lexical
declaration at the same file, line and name.

## Ambiguity and unknowns stay rows

Three rules let a query tell "no" from "not known":

1. **Candidates are rows, with a basis.** A mention whose spelling names
   several declarations has one `mention_target` row per candidate, with
   `basis` set to `candidate`. A query that joins through `mention_target`
   and drops `basis` turns every candidate into a claim. Keep the column, or
   require `basis = "unique"`.
2. **Every resolved relation names its unknowns.** `call` holds only calls
   resolved to exactly one workspace function. Its companion `unknown_call`
   holds every call that was external, ambiguous or unsupported. The same
   holds for `reference` with `unknown_type`, and `mention_target` with
   `mention_unresolved`. The absence of a `call` row is not evidence that no
   call exists while `unknown_call` has rows that could be it.
3. **Observations carry a witness id.** Rows that the other columns cannot
   tell apart (two calls on one line, two identical unresolved type
   mentions) have distinct ids, so a set-based tool does not merge them.
   Ids are unique within one export, not stable between exports.

For example, asking which public functions have no resolved caller counts
every function whose callers Atlas could not resolve as unused. On Atlas's
own source that answer overstates "unused" severalfold. Split it three ways
instead:

```sh
cargo atlas facts --as jsonl --out facts.jsonl
jq -s '
  (map(select(.relation == "call") | {key: .to, value: true}) | from_entries) as $called
  | (map(select(.relation == "unknown_call") | {key: (.spelling | split("::") | last), value: true}) | from_entries) as $spelled
  | map(select(.relation == "function" and .public and (.trait_impl | not))
        | if $called[.id] then "called"
          elif $spelled[.name | split("::") | last] then "possibly called"
          else "no call observed" end)
  | group_by(.) | map({(.[0]): length}) | add' facts.jsonl
```

Even "no call observed" is not proof of no use: calls inside closures and
macro arguments are not observed, and callers outside the workspace are not
read.

Resolved calls between files, counted, which is what `dependencies
--relation calls` draws:

```sh
jq -s '
  (map(select(.relation == "function")) | map({key: .id, value: .path}) | from_entries) as $file
  | map(select(.relation == "call") | {from: $file[.from], to: $file[.to]})
  | map(select(.from != .to))
  | group_by(.) | map(.[0] + {calls: length}) | sort_by(-.calls)' facts.jsonl
```

## Stability

The export carries `schema`, currently 1. Within one schema version, Atlas
only adds relations or appends columns; it never renames, removes or
reorders them, or changes what a column means. Read columns by name (from
`columns`, or the JSON Lines keys), never by a position you counted.

## Codes

A diagnostic's `code` and an unknown call's `kind` are stable: group and
filter by them, not by `message` or `reason`, whose wording may change. Each
list is closed for a release; a code a reader does not know reads as
`unclassified`. `dump`, `census --json` and the other JSON reports carry the
same codes (`code` on each diagnostic, `reasons[].kind` and
`reasons[].subject` on each unknown call).

### Diagnostic codes

| code | meaning |
|---|---|
| `symlink-excluded` | A symbolic link met while walking source directories was not followed. |
| `source-excluded` | A source path outside the workspace, or reached through a symlink. |
| `target-outside-src` | A target root outside `src`; files beside it are not discovered. |
| `mount-missing` | A `mod` declaration whose file is not among the discovered files. |
| `mount-unavailable` | A module file outside `src` that could not be read. |
| `syntax-error` | The file does not parse as Rust. |
| `native-limit` | Written Rust that line-based native readings cannot account for. |
| `use-unreadable` | A `use` declaration whose bindings could not be read. |
| `external-glob` | An external glob import; name-only resolution is suppressed there. |
| `self-unresolved` | `Self` outside any impl or type it could name. |
| `boundary-unjudged` | A boundary path a seam check could not judge. |
| `boundary-declaration-unavailable` | A project boundary declaration that is missing. |
| `boundary-declaration-mismatch` | A project boundary declaration that differs from the captured source. |
| `test-path-unjudged` | A required test path a seam check could not judge. |
| `test-mount-unavailable` | A required test module file that could not be read. |
| `header-citation-stale` | A header-citation observation taken from other source bytes. |
| `header-citation-unavailable` | A required header-citation observation that was not supplied. |
| `measure-unavailable` | A required measurement that could not be made. |
| `population-unavailable` | A selected package without a captured source-tree population. |
| `stage-use-unreadable` | Stage `use` syntax that could not be read. |
| `stage-order-missing` | Stage seams without their required `stage_order`. |
| `seam-import-unavailable` | Required seam import syntax that could not be read. |
| `unclassified` | Read from JSON written before codes existed, or with a code this build does not know. |

### Unknown-call reason kinds

| kind | meaning |
|---|---|
| `reexport-ambiguous` | A public re-export chain that is ambiguous or cyclic. |
| `stage-entry-ambiguous` | A stage's declared entry re-exported ambiguously. |
| `support-api` | A stage's support API, outside its declared entry (subject: stage). |
| `static-call-ambiguous` | More than one workspace function has the path. |
| `foreign-impl-owner` | A trait impl for a type the workspace does not declare (subject: type). |
| `unresolved-path` | A path no workspace function has: external, or not followed (subject: path). |
| `indirect-call` | A call through an expression rather than a path. |
| `value-call` | A call of a local value (subject: name). |
| `local-shadow` | A body-local item or import shadows the module's names (subject: name). |
| `import-namespace-clash` | A local item and an import share the name (subject: name). |
| `import-ambiguous` | The import the path starts with is ambiguous. |
| `self-owner-unknown` | `self`'s type is not established. |
| `receiver-type-unknown` | The receiver has no unique written type (subject: receiver). |
| `receiver-initializer-unknown` | The receiver's initializer result is not established (subject: receiver). |
| `boundary-unresolved` | A boundary call with no resolved entry. |
| `body-unselected` | The call resolved to a body the type facts did not select. |
| `contexts-disagree` | The file's Cargo or module contexts resolve the call to different bodies. |
| `unclassified` | Read from JSON with a kind this build does not know. |

## Relations

### `file`

Lexical. One package's observation of a source file. A path that two packages read has two rows; Atlas never picks an owner.

| column | kind | meaning |
|---|---|---|
| `path` | text | Path relative to the workspace root. |
| `package` | text | The Cargo package that reads the file. |
| `test_only` | boolean | The package reads it only in a test build (a test target, or under a test module). |
| `fingerprint` | text | FNV-1a 64-bit content fingerprint; a change detector, not a security digest. |

- Includes files no module declaration reaches (disconnected source): they have no mount row.

### `mount`

Lexical. A module path at which a package's Cargo target reaches a file through `mod` declarations.

| column | kind | meaning |
|---|---|---|
| `path` | text | The file, as in `file.path`. |
| `package` | text | The package, as in `file.package`. |
| `root` | text | The Cargo target root the path starts from. |
| `module_path` | text | Modules from the target root, joined with `::`; empty for the root file itself. |
| `target` | text | The Cargo target, when several share the root; empty when one target starts there (most rows), since the root then names it. |
| `test_only` | boolean | The target or a module on the path requires a test build. |

- A file can have several mounts (several targets, or `#[path]`), or none.

### `declaration`

Lexical. A declaration site: an item Atlas read at a file and line. Not a resolved Rust symbol.

| column | kind | meaning |
|---|---|---|
| `id` | text | `path:line:name`. A file two packages read gives the same id in each package. |
| `package` | text | The package whose observation of the file this is. |
| `path` | text | The file. |
| `name` | text | The declared name as written (for an impl, its written head). |
| `kind` | text | Fn, Struct, Enum, Union, Trait, TypeAlias, Mod, MacroRules, PubUse, Const, Static, Impl, Use, MacroCall or Extern; an unknown tag is kept verbatim. |
| `visibility` | text | Written visibility: Pub, PubCrate, PubIn, PubSuper or Private. Not Rust's effective visibility. |
| `line` | integer | First line of the item, 1-based. |
| `end_line` | integer | Last line of the item. |
| `inline_modules` | text | Enclosing inline `mod` blocks in the file, outermost first, joined with `::`; empty at file level. |
| `test_only` | boolean | Inside a test module, or in a file the package reads only for tests. |

- The key is (id, package). Count declarations per package observation, as `census` does.

### `import`

Lexical. A `use` declaration as written, per package observation of its file.

| column | kind | meaning |
|---|---|---|
| `path` | text | The file. |
| `package` | text | The package. |
| `written` | text | The `use` tree as written, without resolving it. |

### `mention`

Lexical. A type name written in a declaration's signature or body, matched by spelling.

| column | kind | meaning |
|---|---|---|
| `id` | text | Mention id, unique within one export. |
| `source` | text | The declaration whose text holds the mention (`declaration.id`). |
| `spelling` | text | The identifier as written. |
| `evidence` | text | parameter-mention, return-mention or type-body-mention. |

- Approximate: a spelling match, not compiler name resolution.

### `mention_target`

Lexical. A declaration a mention's spelling can name. An ambiguous mention has one row per candidate.

| column | kind | meaning |
|---|---|---|
| `mention` | text | The mention (`mention.id`). |
| `target` | text | A candidate declaration (`declaration.id`). |
| `basis` | text | `unique` when the spelling names one declaration; `candidate` when it names several and this is one of them. |

Basis column: `basis`.

Unknowns: [`mention_unresolved`](#mention_unresolved) holds what this relation could not resolve.

- A `unique` basis is a unique spelling match, not proof that Rust resolves the name there.

### `mention_unresolved`

Lexical. A mention whose spelling Atlas could not match to a workspace declaration.

| column | kind | meaning |
|---|---|---|
| `mention` | text | The mention (`mention.id`). |
| `reason` | text | Why it is unresolved, as Atlas reports it. |

### `diagnostic`

Lexical. An observation Atlas could not complete while reading the source.

| column | kind | meaning |
|---|---|---|
| `id` | text | Witness id, unique within this relation in one export; not stable between exports. |
| `path` | text | The file. |
| `line` | integer | The line, or 0 when the diagnostic is about the whole file. |
| `package` | text | The package whose observation it is, or empty for the physical file. |
| `message` | text | The diagnostic as written in Atlas's other output; its wording may change. |
| `code` | text | Its kind, a stable code such as `mount-missing` (see the code list). |

### `qualified_file`

Qualified. A production file the qualified analysis read.

| column | kind | meaning |
|---|---|---|
| `path` | text | The file. |
| `fingerprint` | text | Content fingerprint of the bytes analyzed. |

- Production source only: test targets and test modules are outside the qualified population.

### `function`

Qualified. A function body the qualified analysis selected.

| column | kind | meaning |
|---|---|---|
| `id` | text | `path:line:column` of the item. |
| `path` | text | The file. |
| `line` | integer | First line of the item. |
| `name` | text | The name as written, with the impl type for a method (`Type::name`). |
| `qualified` | text | The name with the inline `mod` blocks that enclose it, and the owner type for a method (`Snapshot::graph`); the file's own module path is `mount.module_path`, one per Cargo target that reaches the file. |
| `signature` | text | The signature as written. |
| `public` | boolean | Written `pub` with no restriction. `pub(crate)`, `pub(super)`, `pub(in path)` and private are false, and so is a trait impl's method, which has the trait's visibility. |
| `trait_impl` | boolean | Declared in a trait impl. |
| `declaration` | text | The lexical `declaration.id` at the same file, line and name; empty if none matched. |

### `type`

Qualified. A workspace struct, enum or type alias.

| column | kind | meaning |
|---|---|---|
| `id` | text | `path:line:column` of the item. |
| `path` | text | The file. |
| `line` | integer | First line of the item. |
| `name` | text | The name as written. |
| `qualified` | text | The name with the inline `mod` blocks that enclose it; the file's own module path is `mount.module_path`, one per Cargo target that reaches the file. |
| `alias` | text | For a type alias, its right-hand side as written; empty for a struct or enum. |
| `declaration` | text | The lexical `declaration.id` at the same file, line and name; empty if none matched. |

- An alias is never expanded.

### `field`

Qualified. A field of a workspace struct or enum, with its type as written.

| column | kind | meaning |
|---|---|---|
| `type` | text | The type (`type.id`). |
| `position` | integer | Position among the type's fields, from 0. |
| `name` | text | The field name, or its position for a tuple field. |
| `written` | text | The field's type as written. |

### `call`

Qualified. A call whose target Atlas resolved to exactly one workspace function.

| column | kind | meaning |
|---|---|---|
| `id` | text | Witness id, unique within this relation in one export; not stable between exports. |
| `from` | text | The calling function (`function.id`). |
| `to` | text | The called function (`function.id`). |
| `path` | text | File of the call site. |
| `line` | integer | Line of the call site. |
| `column` | integer | Column of the call site. |

Unknowns: [`unknown_call`](#unknown_call) holds what this relation could not resolve.

- Only unique resolutions: an ambiguous or unresolved call is an `unknown_call` row, never a `call` row.
- Calls inside closures and macro arguments are not observed.

### `unknown_call`

Qualified. A call Atlas could not resolve to exactly one workspace function: external, ambiguous or unsupported.

| column | kind | meaning |
|---|---|---|
| `id` | text | Witness id, unique within this relation in one export; not stable between exports. |
| `from` | text | The calling function (`function.id`). |
| `spelling` | text | The callee as written. |
| `receiver` | text | The method receiver as written, or empty for a path call. |
| `reason` | text | Why the call is unresolved, as Atlas reports it. |
| `path` | text | File of the call site. |
| `line` | integer | Line of the call site. |
| `column` | integer | Column of the call site. |
| `kind` | text | Each reason's stable kind, such as `unresolved-path`, joined with `; ` like `reason`. |
| `subject` | text | Each reason's path, name, receiver, type or module, or empty, joined the same way. |

- Includes calls into other crates and the standard library; an unknown call is not a missing call.

### `reference`

Qualified. A signature or field type that names exactly one workspace type.

| column | kind | meaning |
|---|---|---|
| `id` | text | Witness id, unique within this relation in one export; not stable between exports. |
| `from` | text | The function or type whose signature or field it is. |
| `to` | text | The named type (`type.id`). |
| `role` | text | input, output, field or alias-target. |
| `slot` | text | The parameter, return or field it occupies. |
| `expression` | text | The entire written type, such as `Arc<Graph>`. |
| `line` | integer | Line of the written type. |
| `borrow` | boolean | The written type is a reference. |

Unknowns: [`unknown_type`](#unknown_type) holds what this relation could not resolve.

- `Arc<Graph>` is a reference to Graph, not a claim that the function owns one.

### `unknown_type`

Qualified. A type spelling in a signature or field that Atlas did not resolve to one workspace type.

| column | kind | meaning |
|---|---|---|
| `id` | text | Witness id, unique within this relation in one export; not stable between exports. |
| `from` | text | The function or type whose signature or field it is. |
| `spelling` | text | The type as written. |
| `role` | text | input, output, field or alias-target. |
| `reason` | text | Why it is unresolved, as Atlas reports it. |
| `slot` | text | The parameter, return or field it occupies. |
| `line` | integer | Line of the spelling. |
| `column` | integer | Column of the spelling, 1-based. |

- Includes external and primitive types.
- Each mention is its own row: a generic argument such as the `u32` in `Vec<u32>` is a row of its own beside `Vec<u32>`.

### `construction`

Qualified. A struct literal or enum variant of a workspace type built in a function body.

| column | kind | meaning |
|---|---|---|
| `id` | text | Witness id, unique within this relation in one export; not stable between exports. |
| `from` | text | The function (`function.id`). |
| `to` | text | The type built (`type.id`). |
| `line` | integer | Line of the construction. |

### `default_call`

Qualified. An explicit zero-argument `Type::default()` call for a workspace type.

| column | kind | meaning |
|---|---|---|
| `id` | text | Witness id, unique within this relation in one export; not stable between exports. |
| `from` | text | The function (`function.id`). |
| `to` | text | The type (`type.id`). |
| `line` | integer | Line of the call. |

- No trait dispatch is followed.

### `constant`

Qualified. A written `const` item and its value as syntax; nothing is evaluated.

| column | kind | meaning |
|---|---|---|
| `id` | text | `path::modules::NAME`, unique within one export. |
| `path` | text | The file. |
| `line` | integer | Line of the item. |
| `name` | text | The constant's name. |
| `inline_modules` | text | Enclosing inline `mod` blocks, joined with `::`; empty at file level. |
| `value` | text | The value as JSON: `{"kind": ..., "value": ...}` with kind string, array, tuple, path, call or opaque. |

