//! The score is a measurement of the system, not of the document.
//!
//! The distinction is the whole point of it. A score built from a document's
//! shape — are the expected headings there, are there enough numbered items —
//! can be satisfied in full by a document that is wrong about the code. Every
//! measure here divides what the book accounts for by what the *source*
//! contains, so the only way to move it is to read more of the code or for the
//! code to declare more.

use std::path::{Path, PathBuf};

use nunki_book::{plan, BookOptions};

fn fixtures() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).to_path_buf()
}

fn score_of(fixture: &str) -> nunki_book::score::Score {
    let out = tempfile::tempdir().unwrap();
    plan(&fixtures().join(fixture), out.path(), &BookOptions::default()).unwrap().built.score
}

fn measure<'a>(s: &'a nunki_book::score::Score, name: &str) -> &'a nunki_book::score::Dimension {
    s.dimensions.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("no measure {name}"))
}

/// A repository with typed handlers, traced flows and described entities scores
/// on every measure; the denominators are counts of things found in its source.
#[test]
fn a_service_is_scored_against_what_its_source_contains() {
    let s = score_of("polyglot-shop");
    assert_eq!(s.counted().count(), s.dimensions.len(), "every measure applies to a service: {s:?}");

    let read = measure(&s, "Source read");
    assert_eq!((read.got, read.of), (read.of, read.of), "the fixture is entirely readable");

    // Not an arbitrary threshold: the denominator is the operations the code
    // registers, so this can only move when the fixture's routes change.
    let contract = measure(&s, "Contract declared");
    assert!(contract.of >= 5, "the fixture registers operations to count: {contract:?}");
    assert!(contract.got < contract.of, "the fixture has an undeclared contract, which is the point of it");

    let verified = measure(&s, "Evidence verified");
    assert_eq!(verified.got, verified.of, "a generated book verifies its own citations");

    assert!(s.total.is_some_and(|t| t > 0 && t < 100), "a real repository is neither 0 nor perfect: {s:?}");
}

/// The failure this design exists to avoid: a command-line tool has no HTTP
/// API, so four of six measures have nothing to count. Scoring those as zero
/// would measure the repository's shape; skipping the measure and saying so is
/// the honest answer — and the total must say how many measures it rests on.
#[test]
fn a_tool_is_not_marked_down_for_having_no_http_api() {
    let s = score_of("cli-tool");
    let counted = s.counted().count();
    assert!(counted < s.dimensions.len(), "a CLI cannot be scored on every measure: {s:?}");
    assert!(counted >= 2, "but it can be scored on some: {s:?}");

    for name in ["Behaviour traced", "Data described"] {
        let d = measure(&s, name);
        assert_eq!(d.of, 0, "{name} has nothing to count in a CLI");
        assert_eq!(d.share(), None, "{name} must be skipped, not scored zero");
    }

    // A command is an entry point whose contract the code declares, so the
    // measure that would otherwise be silent about a tool still counts it.
    let contract = measure(&s, "Contract declared");
    assert!(contract.of > 0, "a CLI's commands are its contract: {contract:?}");

    assert!(s.line().contains(&format!("across {counted} of {} measures", s.dimensions.len())), "{}", s.line());
}

/// Two repositories, two shapes, and the measure that separates them is the one
/// about the code rather than the one about the tool. `Source read` is nunki's
/// own limit and says nothing about how well a system is specified.
#[test]
fn the_score_moves_with_the_code_not_with_the_pages() {
    let shop = score_of("polyglot-shop");
    let tool = score_of("cli-tool");
    for s in [&shop, &tool] {
        assert_eq!(measure(s, "Source read").share(), Some(1.0), "both fixtures are fully readable");
    }
    // Same reading, different systems: the totals must still differ, because
    // everything else is counted from what the source declares.
    assert_ne!(shop.total, tool.total, "shop {shop:?} vs tool {tool:?}");
}

/// The number has to travel with the book, or it is a line in a terminal that
/// nobody sees again.
#[test]
fn the_book_states_the_score_and_names_its_weakest_measure() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("polyglot-shop"), out.path(), &BookOptions::default()).unwrap();
    let page = planned.built.book.pages.iter().find(|p| p.id == "evidence").expect("evidence page");
    let text = serde_json::to_string(&page.blocks).unwrap();
    let total = planned.built.score.total.unwrap();

    assert!(text.contains(&format!("{total}% specified")), "the page states the total: {text}");
    for d in planned.built.score.dimensions.iter() {
        assert!(text.contains(d.name), "the page breaks down {}: a total alone cannot be acted on", d.name);
    }
    let weakest = planned.built.score.weakest().unwrap();
    assert!(
        text.contains(&format!("Weakest: {}", weakest.name.to_lowercase())),
        "the page names what is holding the score down: {text}"
    );
    // And it survives into the mirror an agent reads.
    let md = &planned.files["pages/09-evidence-and-unknowns.md"];
    assert!(md.contains(&format!("{total}% specified")), "the Markdown mirror carries the score: {md}");
}
