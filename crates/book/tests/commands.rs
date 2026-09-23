//! The page that says what a command-line tool can be asked to do.
//!
//! Noticed by reading nunki's own published book (#53): it proved the dependency
//! graph across eight crates, and could not say what the tool does — because
//! behaviour was modelled as HTTP operations and a CLI serves none, while
//! `nunki --help` held exactly what a reader wanted.

use std::path::{Path, PathBuf};

use nunki_book::{plan, BookOptions};

fn fixtures() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).to_path_buf()
}

#[test]
fn a_cli_gets_a_command_reference_with_every_argument_cited() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("cli-tool"), out.path(), &BookOptions::default()).unwrap();
    let book = &planned.built.book;

    let page = book.pages.iter().find(|p| p.id == "commands").expect("a CLI gets a command reference");
    let text = serde_json::to_string(&page.blocks).unwrap();

    // The program, its purpose, and each command as the user types it.
    assert!(text.contains("ledger"), "the program name from `#[command(name = …)]`: {text}");
    assert!(text.contains("Reconcile a ledger"), "the about survives the comma inside it");
    for command in ["ledger import", "ledger report", "ledger schema"] {
        assert!(text.contains(command), "{command} is missing: {text}");
    }

    // The arguments, with what happens when they are left out — the thing a
    // reader would otherwise have to read `main.rs` to learn.
    assert!(text.contains("--strict"), "a flag: {text}");
    assert!(text.contains("Fail instead of skipping a row that does not parse."), "the flag's own words");
    assert!(text.contains("positional"), "argument kinds are named");

    // A command taking nothing says so rather than showing an empty table.
    assert!(text.contains("Takes no arguments."), "{text}");

    // Nothing is claimed without a line behind it: every citation the page makes
    // resolves, and lands in the file that declares the commands.
    let cited: Vec<&str> =
        book.cites.values().filter(|c| c.file.ends_with("src/main.rs")).map(|c| c.state.as_str()).collect();
    assert!(!cited.is_empty(), "the declarations are cited");
    // `verified` once the fixture is committed, `untracked` before it is — both
    // mean the cited lines exist and still say what the book claims. What must
    // never appear is a citation that could not be resolved at all, which is
    // what `file-missing`, `line-out-of-range` and `symbol-mismatch` report.
    assert!(cited.iter().all(|s| *s == "verified" || *s == "untracked"), "a citation did not resolve: {cited:?}");

    // And it is reachable, not an orphan page.
    assert!(book.nav.iter().any(|g| g.items.iter().any(|i| i.page == "commands")), "the page is in the navigation");
}

/// A repository that ships no command-line tool gets no page, rather than an
/// empty section explaining that it has no commands.
#[test]
fn a_repository_without_a_cli_gets_no_command_page() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("ts-mini"), out.path(), &BookOptions::default()).unwrap();
    assert!(planned.built.book.pages.iter().all(|p| p.id != "commands"));
    assert!(planned.built.book.nav.iter().all(|g| g.title != "Command reference"));
}

/// The flows page used to tell a CLI repository to edit a diagram and mark a
/// primary path it will never have.
///
/// A command-line tool has no client-to-service chain. Advice that cannot be
/// followed reads as a defect in the repository rather than a fact about it, so
/// the page now says what the entry point actually is and points at it.
#[test]
fn a_cli_is_told_where_its_entry_points_are_not_to_invent_a_primary_path() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("cli-tool"), out.path(), &BookOptions::default()).unwrap();
    let flows = planned.built.book.pages.iter().find(|p| p.id == "flows").expect("a flows page");
    let text = serde_json::to_string(&flows.blocks).unwrap();

    assert!(!text.contains("isPrimaryPath"), "a CLI cannot follow that advice: {text}");
    assert!(text.contains("No request flows"), "{text}");
    assert!(text.contains("command, not a request"), "{text}");
    assert!(text.contains("\"page\":\"commands\""), "and it links to where the commands are: {text}");
}
