# What a source diagram must communicate

The original Girard request was a readable account of how the compiler/runtime
works, located in its actual source files. The purpose was to expose implementation
departures, duplicated mechanisms and coupling—not to reproduce an intended diagram.
The seed supplies questions and expectations, not edges the tool should invent.

Several experiments failed before a useful representation view emerged:

- A complete directory floorplan with thousands of declarations was an unreadable
  inventory. Completeness counts did not establish comprehension.
- A source-browser website answered a different question from a single explanatory
  diagram. Navigation can supplement a figure; it cannot replace its explanation.
- All-file dependency trunks encoded connection density without explaining the
  mechanism. That can be a coupling diagnostic when deliberately aligned for it.
- Adding API lists to those trunks did not fix the abstraction.
- Function signatures became useful when selected around representations and kept
  distinct from calls, retained fields and construction sites. Equal type names do
  not prove runtime value identity, ownership, execution order or termination.

Graphical form is part of the claim. File enclosure means physical source location;
alignment means comparison; arrows need an explicit evidence kind. Do not bundle
unrelated endpoints into a false junction. Curved routes should improve tracing,
not hide ambiguity. Text must remain readable at the intended scale. The default
source-declared overview uses a vertical contract spine because the wider population
made the earlier horizontal figure too small to read.

A sparse matrix can be an honest answer without deserving the default position.
Retention remains available; the default companion view distinguishes returners
from literal construction. Multiple producers are candidates for reading, not an
automatic verdict of unnecessary duplication. Independent reference/native paths
may be required by a system's correctness argument.

Each explanatory view should also survive a relevant controlled departure. Current
regressions include an extra constructor, an executor retaining a semantic graph,
a copied preparation route, enum variants mistaken for types, generic shadowing,
qualified impl owners and line-only changes across revisions. Construction means
observed immediate literals, not exhaustive creation through Default, Clone or macros.

Declared architecture selects and groups the view. Observed relationships remain
independent, including extra constructors and calls outside allowed dependencies.
Unknown and absent are different outcomes. Ambiguity must not become an arbitrary
edge. An extraction test supports only its stated source observation.

Context recovery matters too: preserve the reader's question before implementation
counts and artifact paths. Earlier compactions retained the latter while losing the
former. Peat session 01a15d40, especially turns 410610–412226, 420290 and 433858–441026,
records that drift. The accepted representation experiment is predecessor girard-96b;
its useful source-mutation cases now live in maintained tests. Expression-level
execution/termination recovery is separate work, tracked by girard-8zz, and is not
claimed solved by these contract views.
