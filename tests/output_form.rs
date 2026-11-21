//! `--as` selects the outputs the 0.18 spellings select, byte for byte, and an
//! old spelling prints one notice line on stderr that never changes the answer.
mod support;
use std::process::{Command, Output};
use support::Fixture;

const POLICY: &str = "schema=1\nprovenance='authored'\nsurface={fixture=10}\nauthority=[{fact='one home',home='src/lib.rs::Home'}]\ntyped_errors=['fixture']\n[population]\ncore=['fixture']\n[size]\ncore_token_lines=100\ncrate={fixture=100}\n[crate_graph]\nrule=[{name='fixture',allowed=[],law='leaf'}]\n[concentration]\npopulation=['fixture']\nentry_lines=300\nfile_lines=2000\nconventional_names=[]\n";
const NOTE: &str = "(the old spellings work until 1.0; ATLAS_NOTICES=off hides this)";

/// A committed project with a policy, a saved question and a profile, so
/// every command with `--json` can run on it.
fn fixture() -> Fixture {
    let f = Fixture::new(&[
        (
            "src/lib.rs",
            "/// The home.\npub struct Home;\npub fn run(home: Home) -> Home { keep(home) }\nfn keep(home: Home) -> Home { home }\n",
        ),
        ("policy.toml", POLICY),
        (
            "question.json",
            r#"{"title":"Home","types":[{"name":"Home","file":"src/lib.rs"}]}"#,
        ),
        (
            "profile.json",
            r#"{"schema":1,"source":["src/lib.rs"],"modules":[{"name":"body","kind":"stage","path":"src/lib.rs","entry":"run","types":[],"dependencies":[]}],"workflows":[{"name":"run","path":"src/lib.rs","function":"run"}]}"#,
        ),
    ]);
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=Atlas Test",
            "-c",
            "user.email=atlas@example.invalid",
            "commit",
            "-qm",
            "start",
        ],
    ] {
        let status = Command::new("git")
            .current_dir(f.path())
            .args(args)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    }
    f
}
fn atlas(f: &Fixture, args: &[&str], notices: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"));
    command
        .current_dir(f.path())
        .env("ATLAS_CACHE", "off")
        .args(args);
    if notices {
        command.env_remove("ATLAS_NOTICES");
    } else {
        command.env("ATLAS_NOTICES", "off");
    }
    command.output().expect("run Atlas")
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
/// `text` with `delta`'s elapsed times (`"elapsed_ms": N` and `· Nms`) blanked.
fn untimed(bytes: &[u8]) -> String {
    let text = text(bytes);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find(|c: char| c.is_ascii_digit()) {
        let digits = rest[at..]
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len() - at);
        let (before, after) = (&rest[..at], &rest[at + digits..]);
        out.push_str(before);
        let timed = (before.ends_with("· ") && after.starts_with("ms"))
            || before.ends_with("\"elapsed_ms\": ");
        out.push_str(if timed { "N" } else { &rest[at..at + digits] });
        rest = after;
    }
    out.push_str(rest);
    out
}

#[test]
fn as_json_prints_the_bytes_json_prints_and_json_needs_no_notice() {
    let f = fixture();
    for args in [
        &["orient"][..],
        &["census"],
        &["name", "Home"],
        &["module", "src/lib.rs"],
        &["related", "Home"],
        &["dump"],
        &["query", "?- census{metric: \"files\", n: N}."],
        &["diff", "HEAD"],
        &["moved", "HEAD"],
        &["visibility", "HEAD"],
        &["spans"],
        &["concentration"],
        &["nontest-loc"],
        &["comments"],
        &["dependencies"],
        &["types", "Home"],
        &["compare", "--vs", "HEAD", "--query", "question.json"],
        &[
            "program",
            "--architecture",
            "profile.json",
            "--path",
            "src/lib.rs",
            "--function",
            "run",
        ],
        &["delta", "HEAD", "--policy", "policy.toml"],
        &["stages", "--policy", "policy.toml"],
        &["policy", "--policy", "policy.toml"],
        &["seams", "--policy", "policy.toml"],
    ] {
        let old = atlas(&f, &[args, &["--json"]].concat(), true);
        let new = atlas(&f, &[args, &["--as", "json"]].concat(), true);
        let both = atlas(&f, &[args, &["--json", "--as", "json"]].concat(), true);
        assert!(!old.stdout.is_empty(), "{args:?}: {}", text(&old.stderr));
        assert_eq!(untimed(&old.stdout), untimed(&new.stdout), "{args:?}");
        assert_eq!(untimed(&old.stdout), untimed(&both.stdout), "{args:?}");
        assert_eq!(old.status.code(), new.status.code(), "{args:?}");
        assert!(!text(&old.stderr).contains("note:"), "{args:?}");
        assert_eq!(untimed(&old.stderr), untimed(&new.stderr), "{args:?}");
        if args[0] != "dump" {
            // Text is the default wherever a command has one.
            let plain = atlas(&f, args, true);
            let as_text = atlas(&f, &[args, &["--as", "text"]].concat(), true);
            assert_eq!(untimed(&plain.stdout), untimed(&as_text.stdout), "{args:?}");
        }
    }
    // Plain dump already prints JSON; `--as` offers only that.
    assert_eq!(
        text(&atlas(&f, &["dump"], true).stdout),
        text(&atlas(&f, &["dump", "--as", "json"], true).stdout)
    );
    let refused = atlas(&f, &["dump", "--as", "text"], true);
    assert!(text(&refused.stderr).contains("[possible values: json]"));
}

const FLOW: [&str; 5] = ["flow", "--path", "src/lib.rs", "--function", "run"];

#[test]
fn every_old_output_spelling_prints_what_its_as_form_prints() {
    let f = fixture();
    let flow = FLOW;
    for (old, new) in [
        (
            &["dependencies", "--backend", "ascii"][..],
            &["dependencies", "--as", "text"][..],
        ),
        (
            &["types", "Home", "--backend", "ascii"],
            &["types", "Home", "--as", "text"],
        ),
        (
            &[&flow[..], &["--backend", "ascii"]].concat(),
            &[&flow[..], &["--as", "text"]].concat(),
        ),
        (
            &[
                "map",
                "--architecture",
                "profile.json",
                "--backend",
                "ascii",
            ],
            &["map", "--architecture", "profile.json", "--as", "text"],
        ),
        (&["facts", "--format", "jsonl"], &["facts", "--as", "jsonl"]),
        (&["facts", "--format", "json"], &["facts", "--as", "json"]),
        (&["representations", "Home"], &["types", "Home"]),
    ] {
        let before = atlas(&f, old, false);
        let after = atlas(&f, new, false);
        assert!(before.status.success(), "{old:?}: {}", text(&before.stderr));
        assert_eq!(text(&before.stdout), text(&after.stdout), "{old:?}");
        assert_eq!(text(&before.stderr), text(&after.stderr), "{old:?}");
    }
}

#[test]
fn every_old_image_spelling_writes_what_its_as_form_writes() {
    let f = fixture();
    let flow = FLOW;
    // Images: the same file at the same path, from either spelling.
    for (old, new) in [
        (
            &["dependencies", "--backend", "pele", "--out", "target/d.svg"][..],
            &["dependencies", "--as", "svg", "--out", "target/d.svg"][..],
        ),
        (
            &[
                &flow[..],
                &[
                    "--backend",
                    "pele",
                    "--format",
                    "svg",
                    "--out",
                    "target/f.svg",
                ],
            ]
            .concat(),
            &[&flow[..], &["--as", "svg", "--out", "target/f.svg"]].concat(),
        ),
        (
            &[&flow[..], &["--backend", "pele", "--out", "target/f.html"]].concat(),
            &[&flow[..], &["--as", "html", "--out", "target/f.html"]].concat(),
        ),
    ] {
        let path = f.path().join(old[old.len() - 1]);
        let before = atlas(&f, old, false);
        assert!(before.status.success(), "{old:?}: {}", text(&before.stderr));
        let image = std::fs::read(&path).expect("old spelling wrote the image");
        std::fs::remove_file(&path).expect("clear the image");
        let after = atlas(&f, new, false);
        assert!(after.status.success(), "{new:?}: {}", text(&after.stderr));
        assert_eq!(image, std::fs::read(&path).expect("--as wrote the image"));
        assert_eq!(text(&before.stdout), text(&after.stdout), "{old:?}");
        assert_eq!(text(&before.stderr), text(&after.stderr), "{old:?}");
    }
}

#[test]
fn old_spellings_print_one_notice_that_never_changes_the_answer() {
    let f = fixture();
    let shown = atlas(
        &f,
        &[
            "representations",
            "--type",
            "Home",
            "--backend",
            "ascii",
            "--json",
        ],
        true,
    );
    let hidden = atlas(
        &f,
        &[
            "representations",
            "--type",
            "Home",
            "--backend",
            "ascii",
            "--json",
        ],
        false,
    );
    let stderr = text(&shown.stderr);
    assert_eq!(
        stderr.lines().next(),
        Some(
            format!("atlas: note: --backend is now --as, --type is now the bare TYPES argument, representations is now types {NOTE}")
                .as_str()
        ),
        "{stderr}"
    );
    assert_eq!(stderr.matches("note:").count(), 1, "{stderr}");
    assert!(text(&hidden.stderr).is_empty(), "{}", text(&hidden.stderr));
    assert_eq!(shown.stdout, hidden.stdout);
    assert!(!text(&shown.stdout).contains("note:"));
    assert_eq!(shown.status.code(), hidden.status.code());

    // Several spellings with one replacement share a clause.
    let flow = atlas(
        &f,
        &[
            "flow",
            "--path",
            "src/lib.rs",
            "--function",
            "run",
            "--backend",
            "ascii",
            "--format",
            "html",
        ],
        true,
    );
    assert!(
        text(&flow.stderr).starts_with("atlas: note: --backend and --format are now --as ("),
        "{}",
        text(&flow.stderr)
    );

    // A failing run fails the same way with or without the notice.
    let failing = ["types", "Nowhere", "--backend", "ascii"];
    let (shown, hidden) = (atlas(&f, &failing, true), atlas(&f, &failing, false));
    assert!(!shown.status.success());
    assert_eq!(shown.status.code(), hidden.status.code());
    assert!(text(&shown.stderr).starts_with("atlas: note: --backend is now --as"));
    assert!(text(&shown.stderr).ends_with(&text(&hidden.stderr)));
}

#[test]
fn as_refuses_an_old_spelling_that_asks_for_something_else() {
    let f = fixture();
    for (args, message) in [
        (
            &["census", "--as", "text", "--json"][..],
            "--as text conflicts with --json",
        ),
        (
            &["dependencies", "--as", "svg", "--backend", "ascii"],
            "--as svg conflicts with --backend ascii",
        ),
        (
            &["dependencies", "--as", "text", "--backend", "pele"],
            "--as text conflicts with --backend pele",
        ),
        (
            &[
                "flow",
                "--path",
                "src/lib.rs",
                "--function",
                "run",
                "--as",
                "svg",
                "--format",
                "html",
            ],
            "--as svg conflicts with --format html",
        ),
        (
            &["facts", "--as", "jsonl", "--format", "json"],
            "--as jsonl conflicts with --format json",
        ),
    ] {
        let output = atlas(&f, args, false);
        assert!(!output.status.success(), "{args:?}");
        assert_eq!(
            text(&output.stderr),
            format!("atlas: {message}\n"),
            "{args:?}"
        );
        assert!(output.stdout.is_empty(), "{args:?}");
    }
    // A format the command cannot produce is refused by name.
    let output = atlas(&f, &["census", "--as", "svg"], false);
    assert!(text(&output.stderr).contains("[possible values: text, json]"));
    for (args, values) in [
        (
            &["flow", "--help"][..],
            "[possible values: text, svg, html]",
        ),
        (&["map", "--help"], "[possible values: text, svg, html]"),
        (
            &["program", "--help"],
            "[possible values: text, json, svg, html]",
        ),
        (
            &["dependencies", "--help"],
            "[possible values: text, json, svg]",
        ),
        (&["facts", "--help"], "[possible values: text, json, jsonl]"),
    ] {
        assert!(
            text(&atlas(&f, args, false).stdout).contains(values),
            "{args:?}"
        );
    }
}
