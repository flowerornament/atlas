# Source-grounded explanations

The frontend trial found two separate gaps: named alias handoffs were absent,
and expression mechanics obscured the entry sequence. The repair uses the existing
AST collector, name index, workflow facts and renderer-neutral projection. It adds
no source reader, semantic evaluator or authored arrows.

Named aliases are declarations with their exact written RHS. Their references have
an alias-target role, separate from retained fields and input/output contracts.
Aliases are not expanded, including cycles. Unknown RHS names remain observations.
Producer/preparation selection uses the declared name in the signature. Comparison
reports a changed RHS under that same declaration identity.

The expression-level workflow remains the default. An explicit control detail
folds borrow expressions and generated success/residual bridge nodes. Each fold
requires one incoming and one outgoing edge. Their labels compose in order; every
folded node remains in structured coverage. A residual decision can join its
preceding unexpanded call only when that call has one outgoing edge and the
decision has one incoming edge, in the same physical file. Other branches, guards,
loops, opaque operations, recursion boundaries and matched call returns stay.
This is a contraction of the existing source-control graph, not a second walker.
Control detail refuses above 600 raw nodes before contraction, including at depth
zero; detailed unexpanded views keep their existing behavior. Default expression
views allocate no compaction bookkeeping.

The alternatives were deleting all non-call operations, which can erase effects,
and grouping whole files into flow nodes, which can invent paths. Neither is used.
The research graph search for path-preserving abstraction and semantic zoom found
no directly applicable claim; the local graph conditions and executable controls
are the basis for this bounded projection.

The preparation panel draws direct producer calls to unique function identities,
inside physical-file boxes. Two callers reaching one producer show a shared
implementation without claiming the same runtime instance. Edges show only the resolved producer calls retained by the question; call-site
locations do not assert an evaluation order or a complete execution trace. The textual panel keeps its comparable call-sequence rows. An exact
preparation_functions selection uses the existing name/file selectors, with
refusal on a missing/ambiguous function or one outside the selected return forms.
File selection retains the earlier all-returners behavior. Comparison uses the
same preparation selection; an empty query selection does not silently compare
all preparation routes.

Qualification compares reachability between every retained node in detailed and
compact graphs, including nested residuals, guards, repeated calls, loops and opaque
syntax. A pinned Girard view must also show Forms and both calls to the shared
elaborator implementation. Source facts do not establish runtime object identity,
pass responsibilities or semantic equivalence; those explanations stay identified
as source readings. Human readability remains a human judgment.
