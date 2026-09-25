//! The CLI is a published surface. This compares it against the frozen
//! baseline for the current minor series and fails on a **breaking** change
//! while allowing an additive one.
//!
//! `surface.rs` is a different promise: it fails on *any* difference, which
//! keeps the snapshot honest and puts the change in front of a reviewer. It
//! cannot say whether a consumer would break. A release that only rewrote help
//! text produced a 34-line diff there, indistinguishable at a glance from a
//! renamed flag — so the difference has to be classified, not just detected.
//!
//! `CONTRIBUTING.md` already claimed "the CLI works the same way" as
//! `DiagramIR`. Until this existed it did not: the schema had an executor and
//! the CLI had an invitation to look carefully. Declaring an invariant without
//! one is how a promise ages out quietly.
//!
//! What counts as breaking, from the point of view of something *calling* us —
//! a script, a CI job, an agent that learned the flags once:
//!
//! | breaking                            | additive                       |
//! |-------------------------------------|--------------------------------|
//! | a subcommand disappears             | a new subcommand               |
//! | a flag or positional disappears     | a new flag or positional       |
//! | a flag's value placeholder changes  | a new accepted value           |
//! | a default changes                   | any help or description text   |
//! | an accepted value disappears        |                                |
//!
//! A changed default is breaking even though nothing fails loudly: a caller
//! that relied on `--depth` meaning "container" gets different output rather
//! than an error, which is worse.
//!
//! A command or flag added *after* the baseline was frozen is not yet under the
//! promise: `spec` and `conform` arrived during 0.4 and are guarded from the
//! next freeze onward, the same way `Evidence.repo` sits outside the 0.4 schema
//! baseline. That is the point of freezing rather than comparing against the
//! previous commit — the promise is what was published, not what exists.
//!
//! When a removal is genuinely decided, freeze a fresh baseline in the new
//! minor series — `crates/cli/tests/stable/cli-surface-<x.y>.txt` — and point
//! `BASELINE` at it, exactly as `crates/ir-spec/tests/stability.rs` does.

use std::collections::{BTreeMap, BTreeSet};

const BASELINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/stable/cli-surface-0.4.txt");
const CURRENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/cli-surface.txt");

/// What one command promises: its flags and positionals, each value
/// placeholder, each default, and each accepted value. Prose is deliberately
/// absent — it is the thing that may change freely.
#[derive(Default, PartialEq)]
struct Promise {
    /// `--out <OUT>`, `-o`, `[PATH]`, `<IR>` — name and shape, never help text.
    args: BTreeSet<String>,
    /// `--depth` → `container`, so a silent change of behaviour is caught.
    defaults: BTreeMap<String, String>,
    /// `--format` → {html, svg}; losing one breaks a caller that passes it.
    values: BTreeMap<String, BTreeSet<String>>,
}

fn parse(text: &str) -> BTreeMap<String, Promise> {
    let mut out: BTreeMap<String, Promise> = BTreeMap::new();
    let mut command = String::new();
    // clap wraps long help: the flag is on one line and its `[default: …]` and
    // accepted values are on later ones. Without carrying the subject across
    // those lines, every wrapped entry's default goes unguarded — which is how
    // `nunki spec` and `nunki diff` slipped through the first version of this.
    let mut subject = String::new();
    let mut in_values = false;
    for line in text.lines() {
        if let Some(invocation) = line.strip_prefix("$ nunki") {
            command = invocation.replace("--help", "").split_whitespace().collect::<Vec<_>>().join(" ");
            out.entry(command.clone()).or_default();
            subject.clear();
            in_values = false;
            continue;
        }
        let Some(p) = out.get_mut(&command) else { continue };
        let trimmed = line.trim();

        // Accepted values in the long form: `Possible values:` then `- name: …`.
        if trimmed == "Possible values:" {
            in_values = true;
            continue;
        }
        if in_values {
            match trimmed.strip_prefix("- ") {
                Some(rest) => {
                    let name = rest.split([':', ' ']).next().unwrap_or_default().to_string();
                    if !name.is_empty() && !subject.is_empty() {
                        p.values.entry(subject.clone()).or_default().insert(name);
                    }
                    continue;
                }
                None if trimmed.is_empty() => continue,
                None => in_values = false,
            }
        }

        // A subcommand listed in the `Commands:` block is part of the surface
        // even before its own `--help` block is reached.
        if let Some(rest) = line.strip_prefix("  ") {
            if !rest.starts_with(' ') && !rest.starts_with('-') && !rest.starts_with('[') && !rest.starts_with('<') {
                if let Some(name) = rest.split_whitespace().next() {
                    if command.is_empty() && name != "help" {
                        p.args.insert(format!("command:{name}"));
                    }
                }
            }
        }

        // Flags, their short forms, and the placeholder each one takes.
        for flag in trimmed.split_whitespace().filter(|w| w.starts_with("--") && w.len() > 2) {
            let flag = flag.trim_end_matches(',').to_string();
            let placeholder = trimmed
                .split_once(&flag)
                .and_then(|(_, after)| after.split_whitespace().next())
                .filter(|w| w.starts_with('<'))
                .map(|w| w.trim_end_matches(',').to_string())
                .unwrap_or_default();
            p.args.insert(format!("{flag}{placeholder}"));
        }
        for short in trimmed.split_whitespace().filter(|w| {
            w.len() >= 2 && w.starts_with('-') && !w.starts_with("--") && w.as_bytes()[1].is_ascii_alphabetic()
        }) {
            p.args.insert(short.trim_end_matches(',').to_string());
        }
        // Positionals, which appear alone at the head of an `Arguments:` line.
        if let Some(word) = trimmed.split_whitespace().next() {
            if (word.starts_with('<') && word.ends_with('>')) || (word.starts_with('[') && word.ends_with(']')) {
                p.args.insert(word.to_string());
            }
        }

        // A line naming a flag or positional becomes the standing subject; one
        // that names neither belongs to whatever came before it.
        if let Some(found) = trimmed
            .split_whitespace()
            .find(|w| w.starts_with("--") && w.len() > 2)
            .map(|w| w.trim_end_matches(',').to_string())
            .or_else(|| {
                trimmed
                    .split_whitespace()
                    .next()
                    .filter(|w| (w.starts_with('<') && w.ends_with('>')) || (w.starts_with('[') && w.ends_with(']')))
                    .map(Into::into)
            })
        {
            subject = found;
        }
        if let Some(d) = between(trimmed, "[default: ", "]") {
            p.defaults.insert(subject.clone(), d);
        }
        if let Some(v) = between(trimmed, "[possible values: ", "]") {
            p.values.insert(subject.clone(), v.split(", ").map(str::to_string).collect());
        }
    }
    out
}

fn between(line: &str, open: &str, close: &str) -> Option<String> {
    let start = line.find(open)? + open.len();
    let end = line[start..].find(close)? + start;
    Some(line[start..end].to_string())
}

#[test]
fn the_cli_surface_makes_no_breaking_change_within_this_minor_series() {
    let base = parse(&std::fs::read_to_string(BASELINE).expect("baseline"));
    let now = parse(&std::fs::read_to_string(CURRENT).expect("snapshot"));
    let mut breaking: Vec<String> = Vec::new();

    for (command, was) in &base {
        let Some(is) = now.get(command) else {
            breaking.push(format!("`nunki {command}` is gone"));
            continue;
        };
        for arg in was.args.difference(&is.args) {
            breaking.push(format!("`nunki {command}`: `{arg}` is gone or changed shape"));
        }
        for (arg, old) in &was.defaults {
            match is.defaults.get(arg) {
                Some(new) if new == old => {}
                Some(new) => breaking.push(format!("`nunki {command}` {arg}: default {old:?} became {new:?}")),
                None => breaking.push(format!("`nunki {command}` {arg}: default {old:?} is gone")),
            }
        }
        for (arg, old) in &was.values {
            let gone: Vec<&String> =
                is.values.get(arg).map(|n| old.difference(n).collect()).unwrap_or_else(|| old.iter().collect());
            if !gone.is_empty() {
                breaking.push(format!("`nunki {command}` {arg}: no longer accepts {gone:?}"));
            }
        }
    }

    assert!(
        breaking.is_empty(),
        "the CLI surface broke its promise for this minor series ({} change{}):\n  {}\n\n\
         Additive is free — a new subcommand, a new flag, a new accepted value, any wording. \
         The above is not additive: something that worked against {} stops working.\n\n\
         If the removal is a decision, it belongs in a new minor series with a fresh baseline \
         frozen beside the old one, not in an edit to this test. See CONTRIBUTING.md and issue #9.",
        breaking.len(),
        if breaking.len() == 1 { "" } else { "s" },
        breaking.join("\n  "),
        BASELINE.rsplit('/').next().unwrap_or(BASELINE),
    );
}

/// The gate is only worth having if the baseline describes a real surface.
/// A parser that silently read nothing would pass every assertion above.
#[test]
fn the_baseline_describes_a_surface_worth_guarding() {
    let base = parse(&std::fs::read_to_string(BASELINE).expect("baseline"));
    let commands: Vec<&String> = base.keys().filter(|k| !k.is_empty()).collect();
    assert!(commands.len() >= 10, "baseline parsed only {commands:?}");

    let root = &base[""];
    assert!(root.args.contains("command:generate"), "the root help lists subcommands: {:?}", root.args);

    let generate = base.get("generate").expect("generate");
    assert!(generate.args.contains("--accent<ACCENT>"), "flags carry their placeholder: {:?}", generate.args);
    assert!(generate.args.contains("[PATH]"), "positionals are part of the surface: {:?}", generate.args);
    assert_eq!(generate.defaults.get("[PATH]").map(String::as_str), Some("."), "defaults are read");

    let analyze = base.get("analyze").expect("analyze");
    assert_eq!(
        analyze.values.get("--depth").map(|v| v.iter().cloned().collect::<Vec<_>>()),
        Some(vec!["component".to_string(), "container".to_string(), "system".to_string()]),
        "accepted values are read"
    );
}
