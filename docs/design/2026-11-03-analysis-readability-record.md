# Qualified analysis and move review: reading repair

2026-11-03. Outcome girard-xqe.13, implementing
[the registered plan](2026-11-03-analysis-readability.md) after c72a9a08.

The index entry now reads mounts, binds boundaries, collects functions/exports,
builds names, and binds imported method owners in that order. Cargo contexts,
construction and conservative resolution have separate named homes. Shared lazy
syntax/workflow caches live beside Analysis rather than beneath the index.
The private names CachedSource and retain_sources describe their lazy behavior;
they do not promise an eager parse. Internal consumers use physical owners, while
external core 0.3 compatibility reexports remain the same definitions.

Move review exposes scope, whole-crate inventory, pairing and report construction
at its entry. Its scope, inventory and comparison modules carry those decisions;
tests retain their assertions in a separate file. Workflow separates selectable
function discovery from the one ordered expression traversal. Receiver ownership
now reads as written type, then unique initializer return signature, without a
nested closure chain. The same unknown/ambiguous outcomes and diagnostics remain.

The stage/seam census found existing owners for root bindings, native textual
paths, interfaces, boundaries, writers, views and ratchets. They were kept. No
second evaluator, policy or representation was introduced. No native project pin,
cutover, dependency, public API or schema changed. ARCHITECTURE.md explains both
entry traces and the resulting ownership.

## Evidence

- Full local just check passes: formatting, workspace Clippy/tests, core without
  default features, rustdoc with warnings denied, and both renderer tests. The
  independent external core consumer remains covered by the workspace tests.
- All 58 retained CLI/help/JSON/ASCII/Pele cases are byte-identical to c72a9a08,
  including exit status, stderr and SVG. Fifty-three are successful cases; five
  are retained diagnostic/refusal cases, not five new failures.
- Twenty-one further comparisons are byte-identical, all exiting successfully:
  four reports each on fixed Atlas c72a9a08, Girard 839aa00f and Anneal 2b8e3ba4;
  moved/visibility/diff across a controlled two-revision source at whole-crate,
  current-path and removed-path scope. Comparisons retain actual stdout/stderr.
- An initial comparison trial used the non-module selector `src` and correctly
  refused on both versions. It is retained separately; the accepted comparison
  uses actual source module files, with every case successful.
- Three broad simplification reviews read the changed mechanisms and surrounding
  owners for reuse, quality and efficiency. They found no actionable defect. A
  fresh reader followed both entry workflows without losing their main sequence;
  imported method-owner rebinding's rationale is most explicit in build.rs.
- Early compilation caught extraction mistakes (imports, moved helper path,
  shadowed mount name), and narrowing cache visibility exposed four overly wide
  private helper signatures. Those signatures now expose facts only within
  analysis. These were corrected before the accepted full checks.

Qualification artifacts are consolidated under ignored target/atlas/readability/;
previous syntax-migration evidence is unchanged. Exact publication and remote
receipt custody is recorded on girard-xqe.13.

## Size and limits

The existing production-line instrument, over default-rustfmt source excluding
blank/comment/test lines, reports core 16,064 -> 16,182 (+118; 0.73%), app 6,771
unchanged. Analysis accounts for +98 and queries +20. The combined source changes
from 22,835 to 22,953 (+0.52%). These are navigation costs, not a quality score.
The change is not a deletion claim: new phase boundaries, imports and private
interfaces cost lines. Long expression matching remains one mechanism because
splitting by syntax variant would obscure its ordering rules.

Output parity on fixed inputs and passing tests support behavior preservation;
they do not prove every Rust name resolves or that the architecture is now
universally easy to read. Macro opacity, approximate move keys and unsupported
receiver types keep their documented limits. No performance claim is made.
