//! The CLI is a published surface. This snapshots every subcommand's help so a
//! renamed flag, a changed default or a dropped subcommand fails the build and,
//! more usefully, shows up in review as a diff of the promised surface rather
//! than as a line in a clap derive nobody reads.
//!
//! The subcommand list is read from the top-level help rather than written out
//! here, so adding a command is caught too — it appears in the snapshot diff
//! instead of slipping in unnoticed.
//!
//! Regenerate deliberately, never reflexively:
//!
//! ```text
//! UPDATE_CLI_SURFACE=1 cargo test -p nunki-cli --test surface
//! ```
//!
//! A diff here is a decision about the promise in issue #9, not a chore. Help
//! text carries no version string, so this snapshot is stable across releases.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_nunki");
const SNAPSHOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/cli-surface.txt");

fn help(args: &[&str]) -> String {
    let out = Command::new(BIN).args(args).arg("--help").output().unwrap_or_else(|e| panic!("{args:?}: {e}"));
    assert!(out.status.success(), "`nunki {args:?} --help` failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

/// Subcommand names from the `Commands:` block of the top-level help.
fn subcommands(root: &str) -> Vec<String> {
    root.lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .filter(|n| *n != "help")
        .map(str::to_string)
        .collect()
}

fn render() -> String {
    let root = help(&[]);
    let mut out = format!("$ nunki --help\n{root}\n");
    for cmd in subcommands(&root) {
        out.push_str(&format!("\n$ nunki {cmd} --help\n{}\n", help(&[&cmd])));
    }
    out
}

#[test]
fn the_cli_surface_matches_its_snapshot() {
    let current = render();
    if std::env::var_os("UPDATE_CLI_SURFACE").is_some() {
        std::fs::write(SNAPSHOT, &current).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(SNAPSHOT).unwrap_or_default();
    if committed == current {
        return;
    }
    // A whole-file diff is unreadable in test output; show the first lines that
    // actually differ, which is what a reviewer needs to judge the change.
    let mut shown = 0;
    let mut detail = String::new();
    for (i, (a, b)) in committed.lines().zip(current.lines()).enumerate() {
        if a != b && shown < 10 {
            detail.push_str(&format!("  line {}\n    was: {a}\n    now: {b}\n", i + 1));
            shown += 1;
        }
    }
    if committed.lines().count() != current.lines().count() {
        detail.push_str(&format!("  {} lines before, {} now\n", committed.lines().count(), current.lines().count()));
    }
    panic!(
        "the CLI surface changed:\n{detail}\nIf that is intended, it is a decision about the promise in \
         issue #9 — a renamed flag breaks every workflow pinned to it. Record it, then:\n  \
         UPDATE_CLI_SURFACE=1 cargo test -p nunki-cli --test surface"
    );
}

/// Exit codes are part of the surface: `0` success, `1` verification failed,
/// `2` usage or I/O. A pipeline branches on them, so this pins the behaviour
/// rather than the prose — README.md documents them, and a grep for that
/// sentence would pass just as well against a binary that had stopped obeying
/// it. `--help` deliberately does not list them: adding it would change the
/// very surface this file freezes.
#[test]
fn the_documented_exit_codes_are_the_ones_the_binary_returns() {
    let code = |args: &[&str]| -> i32 { Command::new(BIN).args(args).output().unwrap().status.code().unwrap_or(-1) };
    assert_eq!(code(&["--help"]), 0, "0 is success");
    assert_eq!(code(&["no-such-subcommand"]), 2, "2 is usage");
    assert_eq!(code(&["verify", "does/not/exist.rs:1-2"]), 1, "1 is verification failed");

    // And the sentence a caller actually reads has to keep listing all three.
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap();
    let line =
        readme.lines().find(|l| l.starts_with("Exit codes:")).expect("README.md no longer documents the exit codes");
    for c in ["`0`", "`1`", "`2`"] {
        assert!(line.contains(c), "README exit-code line is missing {c}: {line}");
    }
}
