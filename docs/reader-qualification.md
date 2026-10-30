# Reader qualification, 2026-10-27

The first exercise used Girard source at
`e9e103820a5036513852f8defed2d0ad30f0fa1b`, with comparison against
`57d45f8036c4466fd8b80a7e6ff785ee6af4c77f`. A fresh reader inspected the
PNG figures first, then the JSON evidence and pinned source. This is a proxy
reader exercise, not user visual acceptance or a timing study.

- **Where is Graph created?** The producer figure correctly distinguished seven
  returners from the immediate literal in `verify/calibrate.rs`, but missed the
  practical answer that elaboration initializes `Graph::default()` in
  `frontend/elaborate/eval.rs`. The revised figure lists explicit zero-argument
  `Type::default()` calls separately from literals. It does not infer trait
  dispatch, freshness, inferred `Default::default()`, Clone or macro construction.
  Full paths replace ambiguous basenames; column headings occupy the data cells
  rather than a second full-height header row.
- **What reaches admission?** `&Checked` and `Plan` enter; the success form is
  `AdmittedPlan`, followed by emission to `Executable`. The success-only drawing
  concealed `Result<_, Box<Refused>>`. Resolved workspace error forms now accompany
  the success contract even when not selected as main representations. Labels say
  `Ok`, `Err` and `elements`; JSON retains exact type positions. Unresolved error
  forms remain an extraction limit, not an implied total function.
- **What changed?** Nine observations described deletion of one tooling file,
  `xtask/src/profile.rs`; they were not nine behavioral changes. The old horizontal
  graph was too small. Comparison now uses aligned rows, exact revision labels,
  visible added/removed/changed status, readable field values and an explicit
  empty producer-call sequence. It does not invent a behavioral summary from the
  observations. Identifiers and filenames are not split arbitrarily to wrap text.

Regression cases cover omitted default routes, generic/deferred/inferred defaults,
unselected refusal forms, comparison framing and compatibility of lexical commands.
Generated figures and their source evidence live under ignored `target/atlas/`.
The ordinary librsvg rasterizer does not resolve Pele's CSS custom properties;
visual PNG previews must resolve the declared fallback colors first. That is a
preview limitation, not evidence of black labels in the native browser SVG.

## Interface census

Repository consumers were checked in CLI tests, installed-binary checks, README,
saved queries and Girard's recipe/usage guide. `related` duplicates `name` with
relationships expanded: `name --relations` is now canonical, with a hidden warning
alias preserving existing output. No separate relationship implementation was added.

Other commands answer distinct questions: `dump` exposes the complete lexical model;
`map` projects quotient/declaration permissions or lexical relationships;
`dependencies` projects qualified calls/signature/field observations. Their evidence
contracts remain distinct. No external consumer census is claimed. Representation
population caps apply before rendering, including ASCII/JSON; documentation now
states this rather than suggesting a nonexistent bypass.

## Source-control increment

The existing workflow reader now exposes operand evaluation before the enclosing
operation, each residual `?`, match pattern/guard/body structure, loop headers,
labelled transfers and return operands. `flow --depth N` uses the existing source
name index to expand callees by occurrence. Each expanded body has a distinct
return continuation; files are containers, not merged flow vertices. Unavailable
bodies, recursion and depth boundaries stay annotated at their call sites.

Executable witnesses reproduce the nested residual and failed-pattern guard
examples preserved on Atlas #1. Graph reachability checks ensure the corresponding
residual/pattern-failure branches skip later work. Other cases cover repeated
cross-file calls, loop exhaustion, condition re-evaluation, infinite loops without
invented normal exits, recursion and the node-budget refusal. These checks support
ordinary source-control structure; they do not establish path feasibility or infer
panic, unwinding, destructor, overloaded-operation or macro execution.

The broad simplify review found duplicate parsing in expanded mode and a node-budget
hole for empty match arms. Expansion now reuses its indexed root workflow for output,
resolves each function's call sites once, and refuses the budget at node creation.
The producer review also required keeping the `Err` position visible when an error
type itself is queried.

Figures were generated at the same pinned Girard input for `main`, `render`,
`compile_with`, `menu` and `run_executable`. Expanded helper bodies can still make
large functions much too tall: `render` at depth 1 was about 48,570 SVG units high.
This misses the goal of a readable whole-program explanation. Expansion therefore
remains opt-in, the default overview remains contracts, and Atlas #1 stays open for
readable abstraction of the control facts. A structurally qualified graph is not
automatically a communicative diagram. No whole-program visual acceptance is claimed.
