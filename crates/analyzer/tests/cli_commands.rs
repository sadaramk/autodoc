//! Integration: what a command-line tool can be asked to do.
//!
//! A book about a CLI proved its dependency graph and said nothing about the
//! tool, because behaviour was modelled as HTTP operations and a CLI serves
//! none — while `clap`'s derive declarations carry the same kind of contract a
//! route table does, on citable lines. Noticed by reading nunki's own published
//! book; see #53.

use std::path::{Path, PathBuf};

use nunki_analyzer::cli::{ArgKind, CliSurface};
use nunki_analyzer::{scan, ScanOptions};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn surfaces(name: &str) -> Vec<CliSurface> {
    scan(&fixture(name), &ScanOptions { behavior: true, ..Default::default() }).unwrap().cli
}

#[test]
fn a_clap_derive_surface_is_read_as_commands_and_arguments() {
    let cli = surfaces("cli-tool");
    assert_eq!(cli.len(), 1, "one program: {cli:#?}");
    let c = &cli[0];

    assert_eq!(c.program, "ledger", "`#[command(name = …)]`, not the crate name");
    assert_eq!(
        c.about.as_deref(),
        Some("Reconcile a ledger, and report what does not balance"),
        "the about survives the comma inside it"
    );

    let names: Vec<&str> = c.commands.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, vec!["import", "report", "schema"], "every variant, spelled as clap spells it");

    let import = &c.commands[0];
    assert_eq!(import.about.as_deref(), Some("Import entries from a statement file."));
    assert!(import.evidence.file_path.ends_with("src/main.rs"), "cited to the declaration");

    // Kind, type, default and the argument's own description.
    let path = &import.args[0];
    assert_eq!((path.name.as_str(), path.kind), ("PATH", ArgKind::Positional));
    assert_eq!(path.default.as_deref(), Some("."), "a default is what happens when it is left out");
    assert!(!path.required, "an argument with a default is not required");

    let strict = &import.args[1];
    assert_eq!((strict.name.as_str(), strict.kind), ("--strict", ArgKind::Flag));
    assert_eq!(strict.doc.as_deref(), Some("Fail instead of skipping a row that does not parse."));
    assert!(!strict.required, "a flag is absent by default");

    let out = &import.args[2];
    assert_eq!((out.name.as_str(), out.kind), ("--out", ArgKind::Option));
    assert_eq!(out.type_name, "Option<PathBuf>");

    // A required positional: no default, not optional.
    let account = &c.commands[1].args[0];
    assert_eq!((account.name.as_str(), account.kind), ("ACCOUNT", ArgKind::Positional));
    assert!(account.required, "a bare positional has to be given");

    // A command that takes nothing is still a command.
    let schema = &c.commands[2];
    assert!(schema.args.is_empty(), "{:?}", schema.args);
    assert_eq!(schema.about.as_deref(), Some("Print the schema and exit."));
}

/// A repository that ships no command-line tool says so by having nothing here,
/// rather than by an empty section in its book.
#[test]
fn a_repository_without_a_cli_has_no_command_surface() {
    assert!(surfaces("ts-mini").is_empty());
    assert!(surfaces("go-mini").is_empty());
}
