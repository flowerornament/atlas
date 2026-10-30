# Extending Atlas

Atlas separates source observations, project declarations, view models and output.
Its Rust library supports embedding. Independently installed declaration adapters
use a small JSON protocol; they do not link into the running CLI.

## Use a declaration adapter

```sh
atlas overview /path/to/project --adapter /path/to/my-adapter --backend pele
atlas overview /path/to/project --adapter /path/to/my-adapter --json
```

The adapter path is explicit. Atlas does not execute arbitrary commands discovered
in project configuration or download plugins. When `--adapter` is omitted, the
compatibility default is the official `atlas-girard` executable installed beside
`atlas`. Install the complete package to obtain both binaries; an installation
restricted to `--bin atlas` cannot provide the default adapter.

Girard's adapter reads MODULES/SURFACES from the source file selected by
`--declarations`, default `src/lib.rs`. It interprets normalized constant syntax
provided by Atlas, without compiling the inspected project or parsing its source
again. A different adapter can interpret another convention.

The accepted declarations choose APIs, groups and permissions. They never supply
observed call edges. The JSON evidence includes the reply, producer name/version,
source snapshot and diagnostics so this distinction remains inspectable.

## Write an adapter

Use the public `adapter::ProjectAdapter` interface and call `adapter::serve` from
the executable's main function. The complete examples are
[`src/bin/atlas-girard.rs`](../src/bin/atlas-girard.rs) and
[`examples/foreign-adapter.rs`](../examples/foreign-adapter.rs). Both use only
public Atlas APIs. The latter interprets a different PIPELINE table with project
labels and a generic support role.

`Request` provides the selected declaration file, normalized constant syntax and
production-file fingerprints/module mounts. `Snapshot::constant` refuses absent
or ambiguous declarations. `Snapshot::module_root` accepts a qualified module path
or a globally unique leaf; leaves do not imply top-level Rust scope. Computed
expressions remain opaque. There is no Rust syntax evaluator in the adapter.

Return `Architecture` with source constant IDs in `provenance`. The host validates
those references and binds the declarations to the project. It checks consistency
and source custody, not the truth of your interpretation. Preserve all ambiguity
and scope limits rather than guessing.

The protocol uses one request/reply on stdin/stdout, version `1`, interface
`project-declarations/v1`, with an echoed input snapshot ID and producer identity.
The reply is declarations or an explicit refusal. stdout must contain only JSON;
stderr is for diagnostics. Request/reply/diagnostic limits are 16 MiB / 2 MiB /
64 KiB. Incompatible, malformed, oversized or stale results refuse before any
diagram is written. Successful diagnostics remain available in the invocation
result and generated evidence. An explicit adapter is trusted executable code,
not a sandboxed process.

For embedding, `adapter::adapt` applies the same interface directly, and
`validate_reply` applies the same reference checks. The process invocation returns
`Invocation { reply, diagnostics }`. These APIs do not install anything or decide
a project's landing policy.

## Implement a rendering backend

`render::View` carries one native model: graph, representations (including producer
tables and retention matrices), comparison rows, or overview. Implement
`Renderer::render_view` for the families you support. Return `Artifact` documents;
do not inspect source files to invent a second analysis.

ASCII and Pele implement these families through the same entry. View-specific
meaning, ordering and source evidence belong to the projection. A renderer must
not convert a table to an edgeless graph to claim support.

Existing graph-only `Renderer::render(&Diagram)` implementations remain valid.
Their default `render_view` accepts graphs and returns typed `UnsupportedView`
errors for other families. Pele honors HTML wrapping while preserving the SVG and
Mermaid companions; ASCII returns text. External renderer processes and arbitrary
new view schemas are outside this first interface.

## Compatibility and limits

The 0.5 release removes the library's Girard-specific `architecture::source::read`;
use the declaration adapter contract instead. Existing CLI default overview usage
is preserved when both package binaries are installed. Existing saved questions,
graph renderers and direct view-rendering helpers remain available.

Source snapshots cover the supplied file manifest/bytes, not an atomic filesystem
transaction. Fingerprints are non-cryptographic and producer names are self-reported.
The underlying Rust scanner's documented limits still apply. This interface does
not qualify replacing Murail's FM/policy or Anneal's audits; see the
[cross-project comparison](integration-comparison.md).

The [design contract](design/extensions.md) records the precise boundary and its
qualification criteria. Independent-consumer and refusal cases live in
`tests/extensions.rs`.
