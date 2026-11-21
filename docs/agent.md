# Atlas for agents

Atlas (`cargo atlas`) answers questions about how a Rust workspace is put
together, from its source, without building or running it. Run it inside the
project, or give `--root PATH`. It never changes the code it reads.

Start every session with `cargo atlas orient`: one page of entry points,
where the code is, the most-called functions, and commands to run next,
built from real names. Most unresolved calls are into other packages, which
Atlas does not resolve; `orient --semantic` (or `--scip FILE`) has an index
tell them apart and counts the callers it adds.

## What an answer means

- **Lexical** answers match written names (`name`, `module`, `census`). Two
  things with one name can be confused; Atlas lists every candidate.
- **Qualified** answers follow `use`, modules and Cargo targets to one item
  (`callers`, `callees`, `dependencies`, `types`, `flow`). What it cannot
  follow is *unresolved* and counted, never dropped.
- **Resolved, possibly, unobserved.** `callers X` lists resolved calls; an
  unresolved call written with X's name is *possibly* a call, counted apart.
  No resolved and no possible caller still does not mean unused: calls in
  macros, through traits Atlas cannot follow, or through function values are
  not recorded.
- Unknown is not absent. An empty list is an observation, not a proof.
- Ambiguity is never resolved by a guess: several matches are each shown.
- **Semantic** readings come from rust-analyzer (SCIP), as separate
  evidence: `--scip FILE` reads an index you made; `--semantic` runs
  rust-analyzer, which runs the project's build scripts and proc macros (use
  it only on code you would build) and can take minutes, then is cached.
  `callers X --semantic` adds the calls the index names
  X for (marked `[semantic]`) and lists as possibly only what neither
  reading pins: most "possibly" rows are ruled out. `query` and `facts`
  read it as the `semantic` family; `dump` and `flow` add it beside theirs.

## Which command

| Question | Command |
|---|---|
| Where do I start? | `cargo atlas orient` |
| Where is X defined, what mentions it? | `cargo atlas name X` |
| Who calls X, what does X call? | `cargo atlas callers X`, `cargo atlas callees X` |
| Which files call which? | `cargo atlas dependencies src` (busiest pairs: the `file_calls` recipe) |
| What is in a file, directory or package? | `cargo atlas module src/lib.rs` |
| What steps does a function take? | `cargo atlas flow --path src/main.rs --function main` |
| What takes or returns a type? | `cargo atlas types Tree,Checked` |
| What changed since a commit? | `cargo atlas diff @-` then `--items`; `moved`, `visibility` |
| Where are the large files? | `cargo atlas concentration`, `cargo atlas spans FILE` |
| Anything else | `cargo atlas query '...'` over `cargo atlas facts` |

`cargo atlas help COMMAND` explains one command; `cargo atlas help topics`
lists the guides (`help concepts`, `help --topic query`, `help --topic facts`).

## Naming what you mean

- The bare argument is the subject. A scope is a path (`src/cli`), a file or
  directory name unique in the project (`parse.rs`), or a package name;
  `--in PATH` and `-p PACKAGE` say which. A directory inside the project is
  always a scope, never another project: use `--root` for that.
- A function or item: a name, `module::name`, `Type::method` (`Snapshot::graph`),
  `FILE:LINE`, or a declaration id from `--as json`.
- A bare name can match hundreds of functions (`new`, `run`, `from`). Narrow
  first with `Type::method`, `FILE:LINE` or `--in PATH`.
- Revisions: in a jj repository `@-` is the previous change and `@--` the
  one before (any jj revision works: a change id, `main`, `trunk()`); in git,
  `HEAD~1`, `HEAD~2`. `diff REV` and `diff --vs REV` are the same.

## Output

- `--as json` (or `--json`) prints the whole answer; text is bounded. Every
  JSON answer has `schema` and says what it selected.
- A text list that leaves rows out ends with `… N more; <command>`: run that
  command for the rest, or `--all`, or `--rows N`.
- Notices go to stderr, one line, never into stdout or JSON.
  `ATLAS_NOTICES=off` hides them.

## Query cookbook

`cargo atlas query 'PROGRAM'` runs Datalog over the facts; `-f FILE` reads a
program from a file (`-` for stdin). `cargo atlas facts --schema` lists every
relation and field; `cargo atlas query --prelude` the named questions. A
count goes in a rule head, and a query ends with `order by` and `limit`
after its last literal, with no comma before them.

Most-called functions, ranked:

```datalog
callers{to: G, n: count(F)} :- calls_function{from: F, to: G}.
?- callers{to: G, n: N}, function{id: G, name: Name} order by N desc limit 10.
```

The busiest file pairs, inside one directory:

```datalog
?- file_calls{from: A, to: B, calls: N}, under(A, "src"), under(B, "src") order by N desc limit 10.
```

Everything `main` reaches through resolved calls (recursion is in `reaches`):

```datalog
?- function{id: M, name: "main"}, reaches{from: M, to: G}, function{id: G, name: Name}.
```

Each function with its module path. `function.qualified` holds only the
inline `mod` blocks and the owner type (`Snapshot::graph`); the file's module
path is in `mount`, one row per Cargo target that reaches the file:

```datalog
?- function{id: F, path: P, name: Name}, mount{path: P, module_path: M, target: T}.
```

Functions of one package:

```datalog
?- function{id: F, path: P, name: Name}, file{path: P, package: "tutorial"}.
```

Is a public function used? Three outcomes, never two. `possible_call` holds
the unresolved calls written with a function's name (`callers` lists them as
"possibly"):

```datalog
called{f: F} :- call{to: F}.
possibly{f: F} :- possible_call{to: F}.
use{name: N, status: "called"} :- function{id: F, name: N, public: true}, called{f: F}.
use{name: N, status: "possibly"} :- function{id: F, name: N, public: true}, !called{f: F}, possibly{f: F}.
use{name: N, status: "unobserved"} :- function{id: F, name: N, public: true}, !called{f: F}, !possibly{f: F}.
?- use{name: N, status: S}.
```

"unobserved" is still not "unused": in the tutorial project `emit` is
unobserved because its one call sits inside `println!`. Atlas refuses a program that negates
calls without reading `unknown_call` (the absence check), and one that drops
a mention's `basis`; `cargo atlas help --topic query` explains both.

## Pitfalls

- `count` goes in a rule head, not a query: `n = count(C)` is refused.
- No comma before `order by`.
- `unknown_call.spelling` is what was written: a method name or a path
  segment, not `Type::method`. Read it with `last_segment`.
- `function.qualified` is not a module path; join `mount` for that.
  `mount.target` is empty when one Cargo target starts at that root, which is
  most rows.
- `function.public` is a bare written `pub`: `pub(crate)` and the like are
  false, and so is a trait impl's method (it has the trait's visibility).
- Before concluding that nothing calls a function, read `possible_call`: the
  unresolved calls written with its name.
- `name X --relations` lists co-mentions, not calls. For calls, use
  `callers X`.
- `dependencies` with no scope draws every package; give a directory to keep
  it readable, or ask `file_calls` for a ranking.
- `diff`, `moved` and `visibility` compare against a committed revision; in
  jj, `@` is the working copy and `@-` its parent.
- `flow` draws one function's body; `--depth N` opens the functions it calls.
