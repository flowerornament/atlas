# Project adapters and view-family backends

Design contract registered 2026-10-27 after the user's go. Work and evidence belong
to bead **girard-bwk** and [Atlas #4](https://github.com/flowerornament/atlas/issues/4).
This document specifies the intended interface; it is not a claim that its
implementation has passed qualification.

## Model, replacement and scope

Atlas derives source observations and presents useful lenses over them. Observed
calls, signatures, fields and construction remain distinct from authored project
declarations. Extensions retain that distinction and its provenance.

This change replaces the Girard declaration grammar inside the library and the
CLI paths that bypass the renderer interface for native tables. It preserves the
existing commands and graph-only renderer entry. It adds a small declaration
adapter contract, an executable transport for that contract, and a renderer entry
that accepts distinct native view families.

The proof does not add a plugin registry, dynamic-library loader, WASM runtime,
scripting language, external renderer process, generic policy engine or automatic
installation. Murail and Anneal source and policy remain unchanged. Public release
of the private repository is a separate decision.

## Source and extension flow

```text
source files -> shared Rust parse -> facts + constant syntax + module mounts
                                      |                     |
                                      |             declaration adapter
                                      |                     |
                                      +---- host checks ----+
                                                |
                                      query / native view family
                                                |
                                         ASCII or Pele
                                                |
                                    artifact + source evidence
```

Project adapters interpret a declaration format. They do not scan source files,
execute the inspected program or manufacture observed calls. Analyses and project
policy can later consume the shared facts, but are not new extension protocols in
this proof. The owning project retains its landing decisions.

## Public declaration interface

- **R1.** The shared AST collector emits written top-level constants with their
  physical source spans and module scope. Supported syntax is string, array, tuple
  and plain path, with parentheses/references normalized. Other expressions remain
  opaque source text; they are never evaluated. Local/deferred constant bodies are
  not attributed to execution of their enclosing function.
- **R2.** `adapter::Snapshot::from_facts` selects the declaration file's constants
  and the shared production-file manifest/module mounts. It walks no filesystem
  tree and parses no Rust. Its content identity covers this input, including file
  fingerprints. FNV fingerprints detect source drift; they are not security hashes.
- **R3.** `ProjectAdapter` consumes a typed request and returns architecture
  declarations. Compiled callers and executable adapters use this same contract.
  Every accepted declaration result names source constants in its provenance.
  Absent/ambiguous constants or module roots refuse, rather than selecting one.
- **R4.** Module operational roles remain separate from optional project display
  labels. A generic support role is available; another project need not describe
  its modules as Girard vocabulary/shared laws. Existing role spellings remain
  compatible. Display labels never become observed dependencies.

The official adapter is `src/bin/atlas-girard.rs`, compiled as **atlas-girard**. It
uses only public Atlas APIs and owns the MODULES/SURFACES tuple interpretation.
It is a separate executable in the existing package, not a new SDK workspace or
an exporter restored inside Girard. Copying it into an independent Cargo consumer
must compile and run without private imports or scanner code.

## Executable protocol and custody

- **R5.** One JSON request/result exchange uses protocol `1` and interface
  `project-declarations/v1`. The request carries the declaration source and input
  snapshot. The reply carries protocol/interface, the same snapshot identity,
  producer name/version, and either declarations or an explicit refusal.
- **R6.** The host validates protocol/interface, snapshot identity, producer fields
  and declaration-source references. It rechecks file fingerprints before and
  after invocation, then binds accepted declarations to the project. Missing
  executables, process failure, malformed replies, incompatibility, invented source
  references and source drift produce errors before artifact writing.
- **R7.** Request, reply and diagnostic limits are respectively 16 MiB, 2 MiB and
  64 KiB. stdout is JSON; stderr is diagnostics. All three pipes are serviced
  concurrently, and excess output is drained without retaining unbounded buffers.
  No shell, automatic downloading or installation is involved. An explicit user
  executable is ordinary trusted code, not a sandboxed plugin; cancellation uses
  normal process behavior, not a new scheduler or timeout framework.

Custody checks establish consistency of references and the source version. They
do not prove the adapter's interpretation correct or authenticate its claimed
producer identity. Adapter provenance must remain visible in generated evidence.

`atlas overview ROOT --adapter PATH` selects an executable explicitly. Without it,
the compatibility default is the official **co-installed** atlas-girard binary.
The CLI does not search arbitrary project configuration for executable commands.
`--declarations FILE` selects the normalized source path, default `src/lib.rs`.
Stock Atlas must use an independently built adapter without recompilation.

## Renderer contract

- **R8.** `render::View` distinguishes graph, representations, comparison and
  overview models. It does not coerce rows or matrices into edgeless graphs.
  `Renderer::render_view` is the common entry used by the native-view CLI commands.
- **R9.** Existing `Renderer::render(&Diagram)` clients and implementations remain
  valid. Its default `render_view` delegates graphs and returns typed
  `UnsupportedView` errors for other families. ASCII and Pele implement all current
  families; a third-party backend can support a subset explicitly.
- **R10.** Backends consume selected view models, not source or project policy.
  Their artifacts preserve the same evidence meanings and unknowns. ASCII returns
  text; Pele returns SVG and Mermaid evidence, optionally HTML wrapping. Existing
  view-specific public rendering functions remain compatibility helpers.

This is an enumerated family interface, not an open-world custom scene graph or a
promise that every future view is already representable. New families require a
demonstrated question and a deliberate interface addition.

## Final code homes and affected consumers

- `representations/collect.rs` and `types.rs`: constant syntax in the existing
  collector. `extract.rs` publishes it with the existing facts.
- `adapter.rs`, `adapter/snapshot.rs`, `adapter/process.rs`: typed embedding,
  custody validation and process transport.
- `src/bin/atlas-girard.rs`: project-specific grammar. The old library
  `architecture::source` reader is removed; this API change is documented for 0.5.
- `architecture.rs`: generic support role and optional display labels. Diagram and
  program projections use those labels without changing relationship evidence.
- `render`: native family dispatch in ASCII/Pele. `overview::ascii` owns the text
  overview formerly assembled in the CLI.
- CLI overview/representation/comparison commands: use the adapter/backend boundary
  and preserve producer/snapshot evidence.
- Installed-binary checks and Girard's one pinned recipe: install both package
  executables. Ordinary Girard compilation still invokes neither.

## Qualification

1. Compare normalized declarations, selected types/APIs and source witnesses on the
   pinned Girard input against the preceding release; preserve controlled unexpected
   constructor/call cases. Rendering changes are judged separately from facts.
2. Build the adapter in an independent Cargo fixture using only public Atlas APIs.
   Run it through an already-built stock CLI. Also exercise a different declaration
   fixture with foreign module labels and a generic support role.
3. Test missing/incompatible/malformed adapters, wrong snapshot IDs, invented source
   witnesses, stale input, explicit refusals and bounded streams. Verify inspected
   source remains unchanged in successful cases and no artifact appears on refusal.
4. Render graph and table families with ASCII/Pele and exercise a graph-only
   external renderer's typed unsupported-family outcome. Source locations and
   uncertainty survive each path.
5. Run `just check`, `just check-install` outside the checkout, and
   `just verify-vendor`; inspect useful standalone SVGs. Publish the qualified Atlas
   commit, then install/smoke its exact pin in Girard and use Girard's normal gate.

Generated evidence stays under ignored `target/atlas/`. No claim of complete Rust
resolution, path feasibility, whole-program visual acceptance or cross-project
replacement follows from these checks.

## Implementation clarifications, 2026-10-27

Recorded after the first targeted tests and broad review, before the pinned Girard
comparison and installed-package qualification:

- Module selectors may be fully qualified Rust module paths or globally unique
  leaves. A leaf such as `admit` intentionally selects `backend::admit`; it does not
  assert top-level Rust scope. Ambiguous leaves refuse. This preserves Girard's
  source convention while allowing another adapter to choose exact paths.
- Custody checks cover the supplied file manifest and known file bytes. They do
  not provide an atomic filesystem transaction or detect every newly added file
  outside that manifest. Producer identity is self-reported, not authentication.
- Successful process diagnostics are returned separately from the wire reply and
  retained/displayed by the CLI; they are not silently discarded or promoted to
  source observations. Internal invocation checks reuse the validated request
  while the public validation functions remain independently safe.
