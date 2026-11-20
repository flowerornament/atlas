# Query: your own questions over the facts

`cargo atlas query` runs a small Datalog program over the relations that
[`cargo atlas facts`](facts.md) (`cargo atlas help --topic facts`) exports. Use it for a question no command
asks: join observations, follow them transitively, negate, and count.

```sh
cargo atlas query '?- file_calls{from: A, to: B, calls: N}, under(A, "src") order by N desc limit 10.'
cargo atlas query -f questions.dl --as json
cargo atlas facts --out target/atlas/facts.json        # capture once,
cargo atlas query --facts target/atlas/facts.json -f questions.dl   # ask many times
```

The answer is rows. A query has no pass or fail: the command exits 0
whenever the program ran, with any number of rows, and 1 when it refused the
program (a mistake in it, an honesty finding, or a limit). An empty answer is
not proof that something does not exist; see [Honesty](#honesty).

## The language

A program is rules and queries. Every relation has named fields, written in
braces; a field you leave out matches anything.

```text
% A comment runs to the end of the line (`#` works too).

% A rule: the head holds when every literal of the body holds.
calls_between{from: A, to: B} :-
    call{from: F, to: G},
    function{id: F, path: A},
    function{id: G, path: B},
    A != B.

% Rules may be recursive.
reaches{from: A, to: B} :- calls_between{from: A, to: B}.
reaches{from: A, to: C} :- reaches{from: A, to: B}, calls_between{from: B, to: C}.

% A fact: a rule with values only.
entry{path: "src/main.rs"}.

% A query: its columns are its variables, in the order they first appear.
?- entry{path: E}, reaches{from: E, to: B} order by B limit 50.
```

- **Values** are text (`"src"`), numbers (`12`) and booleans (`true`,
  `false`). Each field has one kind, from the schema or from the rules that
  define it; a variable read from fields of two kinds is refused.
- **Variables** start with an upper-case letter. `_` matches anything
  without binding, and a variable that starts with `_` (such as `_Call`) is
  bound but not shown in a query's columns. A variable used once in a rule is
  refused as a likely typo: write `_` instead.
- **Negation**, `!relation{...}` or `not relation{...}`, holds when no row
  matches. Its variables must be bound by a positive relation in the same
  rule.
- **Comparisons**: `=`, `!=`, `<`, `<=`, `>`, `>=`. Text compares by its
  characters and numbers by value; booleans take only `=` and `!=`. `X = "a"`
  binds `X` when it is not yet bound.
- **Tests on text**: `under(Path, "src")` (the path is the directory or
  inside it; `""` is the workspace root), `starts_with(T, P)`,
  `ends_with(T, S)`, `contains(T, S)`, and `last_segment(Path, S)`, which
  binds `S` to the part after the last `::`.
- **Counting**: `count(V, ...)` in a head counts distinct bindings of its
  variables for each value of the other head fields:
  `callers{to: G, n: count(F)} :- call{from: F, to: G}.` A group with nothing
  to count has no row. A count is never 0: no row is not a 0.
- **Order and limit** end a query: `order by N desc, A limit 20`. Without
  `order by`, rows are sorted by every column. `total` in the JSON answer
  counts rows before `limit`.

Rows are sets: two derivations of the same values give one row. When you
count observations, count their witness ids (`count(C)` over `call{id: C}`),
not the values they share.

A relation may not depend on itself through a negation or a count; such a
program has no single answer and is refused with the rule that closes the
cycle.

## Relations

The stored relations, their fields and their kinds are those of
[facts.md](facts.md) (`cargo atlas help --topic facts`), printed by `cargo atlas facts --schema`. A program
reads only the families it needs, so a lexical-only program does not run the
slower qualified analysis.

The **prelude** (`cargo atlas query --prelude`) adds relations that restate
commands, each kept equal to its command by a test:

| relation | fields | restates |
|---|---|---|
| `file_call` | `from`, `to`, `call` | one resolved call between two files, with its witness id |
| `file_calls` | `from`, `to`, `calls` | `dependencies --relation calls`: file edges and their call counts; restrict both ends with `under` for a scope |
| `name_match` | `name`, `declaration` | the production declarations `name NAME` matches |
| `calls_function` | `from`, `to` | `callers` and `callees`: one step, a function with at least one resolved call to another |
| `reaches` | `from`, `to` | `callers --depth` and `callees --depth`: the functions reached through resolved calls at any depth |
| `name_card` | `name`, `declaration`, `relation`, `mention` | `name NAME --relations`: `outgoing`, `incoming` and `possible` mentions |
| `production_path` | `path` | a path some package reads outside a test build |
| `census` | `metric`, `key`, `n` | `census`, as one row per metric (`key` is the kind or package for `kinds` and `crates`); a metric with nothing to count has no row |

A program cannot add rules to a stored or prelude relation; name your own.

## Honesty

Two checks refuse a program whose rows would claim more than the facts
observed. Each finding quotes the rule and its line and column.

- **basis**: `mention_target` names one row per candidate, with a `basis`
  of `unique` or `candidate`. A rule that reads it must keep `basis` in its
  head or pin it (`basis: "unique"`, or `B = "unique"`). A head field filled
  from a basis is a basis too, so dropping it in a later rule is caught.
- **absence**: negating a relation derived from `call`, `reference` or
  `mention_target` reads "no resolved row" as "none". Their unresolved
  observations are in `unknown_call`, `unknown_type` and
  `mention_unresolved`, and one of them could be the missing row. A rule that
  negates such a relation must also read its unknowns relation.

A question about use therefore has three outcomes, not two:

```text
called{f: F} :- call{to: F}.
maybe{name: S} :- unknown_call{spelling: W}, last_segment(W, S).
use{name: N, status: "called"} :- function{id: F, name: N, public: true}, called{f: F}.
use{name: N, status: "candidate"} :-
    function{id: F, name: N, public: true}, !called{f: F}, last_segment(N, S), maybe{name: S}.
use{name: N, status: "unobserved"} :-
    function{id: F, name: N, public: true}, !called{f: F}, last_segment(N, S), !maybe{name: S}.
?- use{name: N, status: S}.
```

`unobserved` means Atlas saw neither a resolved call nor an unresolved one
with that name. It is still not proof that the function is unused: calls
through traits, macros and function values may not be recorded at all.

`--accept basis` or `--accept absence` runs a program despite that check's
findings; the answer lists each accepted finding. The checks catch an
omission, not every misuse: a rule that reads the unknowns has accounted for
them, whether or not it reads them well.

The answer's notes name each stored relation the program read without its
unknowns relation, and the qualified family's limitations when it read
qualified facts.

## Saved facts

`--facts FILE` evaluates over an export written by `cargo atlas facts`
(JSON or JSON Lines) instead of capturing now. The export must have this
Atlas's schema version and every column. Atlas compares the export's file
fingerprints with the tree and notes how many files have changed since: the
rows still describe the files as they were.

## Limits

Evaluation refuses, naming the relation that grew, when the rules derive
more than `--max-rows` rows (20,000,000 by default) or run longer than
`--timeout-seconds` (120). A recursion over every pair of functions can
grow quickly; start it from the rows you care about.

## Output

Text output prints each query's rows under its columns, at most `--rows N`
rows (20) per query, ending a cut table with `… N more;` and the same command
with `--all`; then the accepted findings and notes. `--as json` (or `--json`)
prints
`schema`, `facts` (where the rows came from), `tables` (each with `query`,
`columns`, `kinds`, `rows` and `total`), `derived`, `elapsed` (milliseconds),
`accepted` and `notes`. `--as jsonl` prints that document without the rows
on its first line, then one `{"table": i, "row": [...]}` line per row.

The rules choose what they read, so `--in`, `-p`, `--vs` and `--at` are
refused. The 0.18 spelling `--show N` still works until 1.0, with a note on
stderr.

## From a library

```rust
use atlas_lib::query::{self, Limits, Options};

let program = query::parse("?- file_calls{from: A, to: B, calls: N}.")?;
let checked = query::check(&program, &relations.families, &Options::default())?;
// checked.families() names the families to capture, if not yet captured.
let answer = query::run(&checked, &relations, Limits::default())?;
```
