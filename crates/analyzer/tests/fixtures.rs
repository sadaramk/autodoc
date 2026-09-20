//! Integration: the analyzer against real fixture repositories. Every piece
//! of evidence it reports must point at lines that actually contain what it
//! claims — checked by reading the files back.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use autodoc_analyzer::scan::{EvidenceRef, RelationSource};
use autodoc_analyzer::{draft_ir, scan, Depth, DraftOptions, ScanOptions, ScanReport, UnitKind};
use autodoc_ir::EdgeType;

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn scan_at(path: &Path, depth: Depth) -> ScanReport {
    scan(path, &ScanOptions { depth, ..Default::default() }).unwrap()
}

fn assert_evidence_real(root: &Path, e: &EvidenceRef) {
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        e.start_line >= 1 && e.end_line as usize <= lines.len() && e.start_line <= e.end_line,
        "{e:?} out of range"
    );
    if let Some(sym) = &e.symbol_name {
        let window = &lines[e.start_line as usize - 1..e.end_line as usize];
        let bare = sym.trim_start_matches("__");
        assert!(window.iter().any(|l| l.contains(bare) || l.contains("if __name__")), "{sym} not within {e:?}");
    }
}

#[test]
fn polyglot_shop_container_model() {
    let root = fixture("polyglot-shop");
    let r = scan_at(&root, Depth::Container);
    let kinds: Vec<(&str, UnitKind)> = r.containers.iter().map(|u| (u.id.as_str(), u.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            ("api-gateway", UnitKind::HttpService),
            ("fulfillment", UnitKind::Worker),
            ("ledger-audit", UnitKind::HttpService),
            ("payments", UnitKind::HttpService),
            ("web", UnitKind::WebClient),
        ]
    );
    let langs: BTreeSet<&str> = r.stats.languages.keys().map(String::as_str).collect();
    assert_eq!(langs, BTreeSet::from(["Go", "Python", "Rust", "TypeScript"]));
    assert_eq!(r.stats.files_with_parse_errors, 0);

    let infra: Vec<&str> = r.infrastructure.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(infra, vec!["postgres", "redis", "kafka", "stripe", "sendgrid"]);
    let kafka = r.infrastructure.iter().find(|i| i.id == "kafka").unwrap();
    assert_eq!(kafka.topics, vec!["order.placed"]);
    assert_eq!(kafka.compose_service.as_deref(), Some("kafka"));

    let rel = |s: &str, t: &str| {
        r.relationships.iter().find(|x| x.source == s && x.target == t).unwrap_or_else(|| panic!("{s} → {t} missing"))
    };
    assert_eq!(rel("web", "api-gateway").edge_type, EdgeType::Sync);
    assert_eq!(rel("api-gateway", "payments").label, "HTTP");
    assert!(rel("api-gateway", "payments").sources.contains(&RelationSource::Code));
    assert_eq!(rel("api-gateway", "postgres").edge_type, EdgeType::Write);
    assert_eq!(rel("api-gateway", "postgres").label, "writes orders");
    assert_eq!(rel("ledger-audit", "postgres").edge_type, EdgeType::Read);
    assert_eq!(rel("api-gateway", "kafka").label, "publishes order.placed");
    assert_eq!(rel("kafka", "fulfillment").edge_type, EdgeType::Event);
    assert_eq!(rel("payments", "stripe").label, "charges cards");
    assert_eq!(rel("fulfillment", "sendgrid").label, "sends email");
    assert!(
        !r.relationships.iter().any(|x| x.source == "fulfillment" && x.target == "kafka"),
        "consumer must not point at the bus"
    );

    // The payments call is pinned to the function that makes it, not to compose.
    let http = &rel("api-gateway", "payments").evidence[0];
    assert_eq!(http.file_path, "api-gateway/src/clients/payments.ts");
    assert_eq!(http.symbol_name.as_deref(), Some("chargeOrder"));

    for e in r.relationships.iter().flat_map(|x| &x.evidence).chain(r.evidence_map.values()) {
        assert_evidence_real(&root, e);
    }
    for u in &r.containers {
        assert!(!u.entry_points.is_empty(), "{} has no entry point", u.id);
        for s in &u.key_symbols {
            assert_evidence_real(&root, &s.evidence);
        }
    }
}

#[test]
fn polyglot_shop_drafts_are_valid_at_every_depth() {
    let root = fixture("polyglot-shop");
    for depth in [Depth::System, Depth::Container, Depth::Component] {
        let r = scan_at(&root, depth);
        let d = draft_ir(&r, &DraftOptions { generated_at: Some("2026-01-01T00:00:00Z".into()), ..Default::default() });
        let v = autodoc_validator::validate(
            &d.ir,
            &autodoc_validator::ValidateOptions { repo_root: Some(root.clone()), ..Default::default() },
        );
        assert!(v.valid, "{depth:?}: {:#?}", v.diagnostics);
        assert_eq!(v.warning_count, 0, "{depth:?}: {:#?}", v.diagnostics);
        assert_eq!(d.ir.nodes.iter().filter(|n| n.is_key_focal_point).count(), 1);
        assert!(d.ir.edges.iter().any(|e| e.primary()));
    }
    let r = scan_at(&root, Depth::Container);
    let d = draft_ir(&r, &DraftOptions::default());
    let primary: Vec<(&str, &str)> =
        d.ir.edges.iter().filter(|e| e.primary()).map(|e| (e.source.as_str(), e.target.as_str())).collect();
    assert_eq!(primary, vec![("api-gateway", "payments"), ("payments", "stripe"), ("web", "api-gateway")]);
    assert!(d.ir.node("api-gateway").unwrap().is_key_focal_point);
}

/// (fixture, focus unit, module edges that must be resolved)
type ComponentCase = (&'static str, Option<&'static str>, &'static [(&'static str, &'static str)]);

#[test]
fn component_view_resolves_imports_in_each_language() {
    let expect: &[ComponentCase] = &[
        ("rust-mini", None, &[("rust-mini.main", "rust-mini.tokenizer"), ("rust-mini.report", "rust-mini.tokenizer")]),
        ("ts-mini", None, &[("ts-mini.index", "ts-mini.routes"), ("ts-mini.routes", "ts-mini.store")]),
        (
            "go-mini",
            None,
            &[("go-mini.main", "go-mini.internal-handlers"), ("go-mini.internal-handlers", "go-mini.internal-store")],
        ),
        ("python-mini", None, &[("python-mini.main", "python-mini.app"), ("python-mini.routes", "python-mini.models")]),
        (
            "polyglot-shop",
            Some("payments"),
            &[
                ("payments.cmd-payments", "payments.internal-charge"),
                ("payments.internal-httpapi", "payments.internal-ledger"),
            ],
        ),
        ("polyglot-shop", Some("fulfillment"), &[("fulfillment.main", "fulfillment.consumer")]),
        (
            "polyglot-shop",
            Some("ledger-audit"),
            &[("ledger-audit.main", "ledger-audit.routes"), ("ledger-audit.routes", "ledger-audit.audit")],
        ),
    ];
    for (name, focus, edges) in expect {
        let root = fixture(name);
        let r = scan(
            &root,
            &ScanOptions { depth: Depth::Component, focus: focus.map(str::to_string), ..Default::default() },
        )
        .unwrap();
        let view = r.components.as_ref().unwrap();
        let deps: BTreeSet<(&str, &str)> =
            view.dependencies.iter().map(|d| (d.source.as_str(), d.target.as_str())).collect();
        for e in *edges {
            assert!(deps.contains(e), "{name}: missing {e:?} in {deps:?}");
        }
        for d in &view.dependencies {
            assert_evidence_real(&root, &d.evidence);
        }
        for m in &view.modules {
            assert_evidence_real(&root, m.evidence.as_ref().expect("fixture modules are non-empty"));
        }
    }
}

#[test]
fn unknown_focus_lists_available_units() {
    let err = scan(
        &fixture("polyglot-shop"),
        &ScanOptions { depth: Depth::Component, focus: Some("nope".into()), ..Default::default() },
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("nope") && msg.contains("api-gateway"), "{msg}");
}

#[test]
fn git_context_and_gitignore_are_respected() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("shop");
    copy_dir(&fixture("polyglot-shop"), &repo);
    std::fs::create_dir_all(repo.join("api-gateway/node_modules/pg")).unwrap();
    std::fs::write(repo.join("api-gateway/node_modules/pg/index.js"), "export const x = 1;\n").unwrap();
    std::fs::create_dir_all(repo.join("generated")).unwrap();
    std::fs::write(repo.join("generated/huge.ts"), "export function generated() {}\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "generated/\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["add", "."],
        &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"],
    ] {
        assert!(Command::new("git").arg("-C").arg(&repo).args(args).status().unwrap().success());
    }
    let r = scan_at(&repo, Depth::Container);
    assert!(r.repo.is_git);
    assert_eq!(r.repo.branch.as_deref(), Some("main"));
    assert_eq!(r.repo.commit_hash.as_ref().map(String::len), Some(40));
    assert_eq!(
        r.stats.files,
        scan_at(&fixture("polyglot-shop"), Depth::Container).stats.files,
        "node_modules and gitignored files are skipped"
    );
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// A source file past the size limit is not parsed, so anything it declares is
/// absent from the book. Dropping it silently makes the documentation
/// confidently incomplete, so the scan has to say what it left out.
#[test]
fn oversized_source_files_are_reported_not_dropped_in_silence() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    std::fs::write(repo.join("go.mod"), "module github.com/acme/big\n\ngo 1.22\n").unwrap();
    std::fs::write(repo.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
    let big = format!("package main\n\n// {}\nfunc Huge() {{}}\n", "x".repeat(40_000));
    std::fs::write(repo.join("generated.go"), &big).unwrap();

    let r = scan(repo, &ScanOptions { max_file_bytes: 1_000, ..Default::default() }).unwrap();

    let note = r
        .notes
        .iter()
        .find(|n| n.contains("were not parsed"))
        .unwrap_or_else(|| panic!("no note about the skipped file; notes: {:?}", r.notes));
    assert!(note.contains("generated.go"), "the note should name what was skipped: {note}");
    assert!(note.contains('1'), "the note should count what was skipped: {note}");

    // Under the limit it is parsed as usual, and there is nothing to report.
    let all = scan(repo, &ScanOptions::default()).unwrap();
    assert!(!all.notes.iter().any(|n| n.contains("were not parsed")), "notes: {:?}", all.notes);
}

/// A source file that is not valid UTF-8 was dropped by `.ok()?` with no word
/// anywhere: its routes and entities were simply absent, and the book read as
/// though the repository never contained them. Incompleteness has to be
/// visible, which is what the note about oversized files already does.
#[test]
fn unreadable_source_files_are_reported_not_dropped_in_silence() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    std::fs::write(repo.join("go.mod"), "module github.com/acme/svc\n\ngo 1.22\n").unwrap();
    std::fs::write(repo.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
    // Latin-1 bytes: a real file, valid Go to a compiler, not valid UTF-8.
    std::fs::write(repo.join("legacy.go"), b"package main\n\n// caf\xe9 handling\nfunc Legacy() {}\n").unwrap();

    let r = scan(repo, &ScanOptions::default()).unwrap();

    let note = r
        .notes
        .iter()
        .find(|n| n.contains("could not be read"))
        .unwrap_or_else(|| panic!("nothing said a file was skipped; notes: {:?}", r.notes));
    assert!(note.contains("legacy.go"), "the note should name the file it could not read: {note}");

    // The readable files are still documented.
    assert!(r.stats.files >= 1, "the rest of the repository is still scanned");
}
