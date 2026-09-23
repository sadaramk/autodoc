//! Integration: book structure, determinism, link integrity and citations on
//! every fixture repository.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use nunki_book::model::{Block, Inline};
use nunki_book::{generate, plan, BookOptions};

fn fixtures() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).to_path_buf()
}

const FIXTURES: &[&str] = &[
    "polyglot-shop",
    "rust-mini",
    "ts-mini",
    "go-mini",
    "python-mini",
    "real-world/compose-subdir",
    "real-world/grpc-env-file",
    "real-world/unparsed-worker",
    "real-world/rust-workspace",
    "real-world/uv-workspace",
    "real-world/spring-cloud",
    "real-world/spring-modulith",
    "real-world/k8s-deploy",
    "real-world/go-hexagonal",
    "real-world/go-mux-builder",
    "flows",
    // Data and API shapes that only the analyzer used to see. The information
    // architecture loop below is the only thing that catches a dangling figure
    // id, a missing citation or a broken anchor, and it never ran on these.
    "kotlin/spring-boot-kotlin",
    "kotlin/exposed-ledger",
    "kotlin/webflux-corouter",
    "orm-models/node-app",
    "orm-models/py-app",
    "orm-models/jpa-spring",
    "attribution",
    "client-drift",
    "api-frameworks/quarkus-jaxrs",
    "api-frameworks/micronaut",
];

fn inlines_of(b: &Block) -> Vec<&Inline> {
    match b {
        Block::Para { inl } | Block::Callout { inl, .. } => inl.iter().collect(),
        Block::Table { rows, .. } => rows.iter().flatten().flatten().collect(),
        Block::Figure { caption, .. } => caption.iter().collect(),
        Block::List { items } => items.iter().flatten().collect(),
        Block::Steps { steps, .. } => steps.iter().flat_map(|s| s.title.iter().chain(s.body.iter())).collect(),
        Block::Heading { .. } | Block::Stats { .. } | Block::Cards { .. } => vec![],
    }
}

#[test]
fn every_fixture_gets_the_same_information_architecture_with_intact_references() {
    for name in FIXTURES {
        let out = tempfile::tempdir().unwrap();
        let planned = plan(&fixtures().join(name), out.path(), &BookOptions::default()).unwrap();
        let book = &planned.built.book;
        let ids: Vec<&str> = book.pages.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"overview"), "{name}");
        assert_eq!(ids[1], "architecture", "{name}");
        // Fixed order: system pages, then API reference and product pages when
        // operations exist, evidence always last.
        let data_at = ids.iter().position(|i| *i == "data").expect("data page");
        // Per-domain data pages (large models) directly follow the data overview.
        let domain_pages = ids[data_at + 1..].iter().take_while(|i| i.starts_with("data/")).count();
        let data_at = data_at + domain_pages;
        assert_eq!(ids[data_at + 1], "flows", "{name}");
        assert_eq!(ids.last(), Some(&"evidence"), "{name}");
        let mut tail: Vec<&str> = ids[data_at + 2..ids.len() - 1].to_vec();
        // Runtime & deployment follows the flows when the repository declares an environment.
        if tail.first() == Some(&"runtime") {
            tail.remove(0);
        }
        let api_pages = tail.iter().take_while(|i| i.starts_with("api/")).count();
        let product: &[&str] = if api_pages > 0 { &["functional", "requirements"] } else { &["requirements"] };
        assert_eq!(&tail[api_pages..], product, "{name}: {tail:?}");
        assert!(ids.iter().any(|i| i.starts_with("containers/")), "{name}: at least one container page");

        let page_ids: BTreeSet<&str> = ids.iter().copied().collect();
        for page in &book.pages {
            for block in &page.blocks {
                if let Block::Figure { diagram, .. } | Block::Steps { diagram, .. } = block {
                    assert!(book.diagrams.contains_key(diagram), "{name}/{}: figure {diagram}", page.id);
                }
                if let Block::Cards { cards } = block {
                    for c in cards {
                        assert!(page_ids.contains(c.page.as_str()), "{name}: card → {}", c.page);
                    }
                }
                for inline in inlines_of(block).into_iter().chain(page.summary.iter()) {
                    match inline {
                        Inline::Cite { id } => assert!(book.cites.contains_key(id), "{name}: cite {id}"),
                        Inline::Link { page: p, anchor, .. } => {
                            let target = book.pages.iter().find(|x| x.id == *p);
                            assert!(target.is_some(), "{name}: link → {p}");
                            if let Some(a) = anchor {
                                let found = target
                                    .unwrap()
                                    .blocks
                                    .iter()
                                    .any(|b| matches!(b, Block::Heading { id, .. } if id == a));
                                assert!(found, "{name}/{}: anchor {p}#{a} has no heading", page.id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        for group in &book.nav {
            for item in &group.items {
                assert!(page_ids.contains(item.page.as_str()), "{name}: nav → {}", item.page);
            }
        }
        for (fid, f) in &book.diagrams {
            for target in f.nodes.values() {
                if let Some(p) = &target.page {
                    assert!(page_ids.contains(p.as_str()), "{name}/{fid}: node → {p}");
                }
            }
        }
        // Fixtures aren't git repositories: citations verify against the working tree.
        let bad: Vec<_> = book.cites.values().filter(|c| c.state != "verified").collect();
        assert!(bad.is_empty(), "{name}: {bad:#?}");
        assert!(planned.built.warnings.is_empty(), "{name}: {:?}", planned.built.warnings);
    }
}

#[test]
fn markdown_links_resolve_to_generated_files() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("polyglot-shop"), out.path(), &BookOptions::default()).unwrap();
    for (path, text) in planned.files.iter().filter(|(p, _)| p.ends_with(".md")) {
        let dir = Path::new(path).parent().unwrap_or(Path::new(""));
        let mut rest = text.as_str();
        while let Some(i) = rest.find("](") {
            rest = &rest[i + 2..];
            let target: String = rest.chars().take_while(|c| *c != ')').collect();
            if target.starts_with("http") {
                continue;
            }
            let file = target.split('#').next().unwrap();
            let resolved = normalize(&dir.join(file));
            assert!(planned.files.contains_key(&resolved), "{path}: broken link {target} → {resolved}");
        }
    }
    let llms = &planned.files["llms.txt"];
    for page in &planned.built.book.pages {
        assert!(llms.contains(&page.md_path), "llms.txt lists {}", page.md_path);
    }
}

fn normalize(p: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().into()),
            _ => {}
        }
    }
    parts.join("/")
}

#[test]
fn generation_is_deterministic_and_prunes_files_it_no_longer_produces() {
    let out = tempfile::tempdir().unwrap();
    let repo = fixtures().join("polyglot-shop");
    let first = generate(&repo, out.path(), &BookOptions::default()).unwrap();
    assert!(first.written.len() > 20);
    let second = generate(&repo, out.path(), &BookOptions::default()).unwrap();
    assert!(second.written.is_empty(), "{:?}", second.written);

    // Pretend the previous run produced a page for a service that no longer exists.
    let manifest_path = out.path().join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["files"]["pages/03-containers-legacy.md"] = serde_json::json!("0000");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    std::fs::write(out.path().join("pages/03-containers-legacy.md"), "# Legacy\n").unwrap();
    let third = generate(&repo, out.path(), &BookOptions::default()).unwrap();
    assert_eq!(third.removed, vec!["pages/03-containers-legacy.md".to_string()]);
    assert!(!out.path().join("pages/03-containers-legacy.md").exists());
}

#[test]
fn polyglot_book_content() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("polyglot-shop"), out.path(), &BookOptions::default()).unwrap();
    let book = &planned.built.book;
    let containers: Vec<&str> = book.pages.iter().filter_map(|p| p.id.strip_prefix("containers/")).collect();
    assert_eq!(containers, ["api-gateway", "fulfillment", "ledger-audit", "payments", "web"]);
    let flows = book.pages.iter().find(|p| p.id == "flows").unwrap();
    let steps = flows.blocks.iter().find_map(|b| match b {
        Block::Steps { steps, .. } => Some(steps),
        _ => None,
    });
    let edges: Vec<&str> = steps.expect("primary path walkthrough").iter().map(|s| s.edge.as_str()).collect();
    assert_eq!(edges, ["web--api-gateway", "api-gateway--payments", "payments--stripe"]);
    let arch = &book.diagrams["containers"];
    assert_eq!(arch.nodes["payments"].page.as_deref(), Some("containers/payments"));
    assert!(arch.nodes["postgres"].page.is_none() && arch.nodes["postgres"].cite.is_some());
    let readme = book.cites.values().find(|c| c.file == "README.md").expect("description cites the README");
    assert_eq!((readme.start, readme.snippet.as_deref().map(|s| s.starts_with("A small checkout"))), (3, Some(true)));
    let data = &planned.files["pages/04-data-and-integrations.md"];
    assert!(data.contains("| Service | PostgreSQL | Redis |"), "{data}");
    assert!(data.contains("`order.placed`"), "{data}");
}

#[test]
fn behaviour_pages_document_contracts_flows_data_and_business_intent() {
    let out = tempfile::tempdir().unwrap();
    let repo = fixtures().join("polyglot-shop");
    let planned = plan(&repo, out.path(), &BookOptions::default()).unwrap();
    let book = &planned.built.book;
    let page = |id: &str| book.pages.iter().find(|p| p.id == id).unwrap_or_else(|| panic!("page {id}"));
    let headings = |id: &str| -> Vec<String> {
        page(id)
            .blocks
            .iter()
            .filter_map(|b| if let Block::Heading { text, .. } = b { Some(text.clone()) } else { None })
            .collect()
    };

    // Purpose-built figures, not one view reused everywhere.
    let types: BTreeSet<String> = planned.built.diagrams.iter().map(|d| format!("{:?}", d.ir.diagram_type)).collect();
    for t in ["Sequence", "EntityRelationship", "Lifecycle", "Container", "Component", "SystemContext"] {
        assert!(types.contains(t), "missing {t} figure: {types:?}");
    }
    for d in &planned.built.diagrams {
        let report = nunki_validator::validate(&d.ir, &nunki_validator::ValidateOptions::default());
        assert!(report.valid, "{} does not validate: {:#?}", d.id, report.diagnostics);
    }
    let checkout =
        planned.built.diagrams.iter().find(|d| d.id == "flow-api-gateway-post-checkout").expect("checkout flow");
    let labels: Vec<&str> = checkout.ir.edges.iter().filter_map(|e| e.label.as_deref()).collect();
    assert!(labels.iter().any(|l| l.contains("orders")), "checkout writes orders: {labels:?}");
    assert!(labels.iter().any(|l| l.contains("order.placed")), "checkout publishes order.placed: {labels:?}");
    assert!(checkout.ir.edges.iter().all(|e| e.evidence.is_some()), "every message is pinned");
    // The synchronous reply to the web client, then fulfillment's asynchronous handling of `order.placed`.
    let reply = checkout.ir.edges.iter().position(|e| e.is_reply() && e.target == "web").expect("reply to web");
    let continuation: Vec<(&str, &str)> =
        checkout.ir.edges[reply + 1..].iter().map(|e| (e.source.as_str(), e.target.as_str())).collect();
    assert_eq!(
        continuation,
        [("kafka", "fulfillment"), ("fulfillment", "postgres")],
        "checkout continues into fulfillment"
    );
    assert_eq!(checkout.ir.edges[reply + 1].label.as_deref(), Some("delivers order.placed"));

    // API reference: one page per serving unit, one section per operation.
    let api = headings("api/api-gateway");
    assert!(api.contains(&"POST /checkout".to_string()), "{api:?}");
    assert!(book.pages.iter().any(|p| p.id == "api/payments"));

    // Data: ER model, per-entity column tables, lifecycle.
    let data = headings("data");
    for h in ["Data model", "orders", "Lifecycles", "orders.status"] {
        assert!(data.contains(&h.to_string()), "data page lacks {h}: {data:?}");
    }

    // Functional spec: numbered requirements and rules; intent is a gap, never invented.
    let functional = page("functional");
    let fr = headings("functional");
    // Identified by what they describe, not by position — see
    // `journey_requirement_ids_survive_adding_a_requirement`.
    assert!(fr.iter().any(|h| h.starts_with("FR-") && h.contains("get-catalog")), "{fr:?}");
    assert!(fr.contains(&"Business rules".to_string()));
    let text = serde_json::to_string(&functional.blocks).unwrap();
    assert!(text.contains("needs input: actor"));
    assert_eq!(text.matches(r#""tone":"authored""#).count(), 1, "only the legend: nothing authored yet");

    let reqs = headings("requirements");
    assert_eq!(reqs.len(), 9, "{reqs:?}");
    assert!(serde_json::to_string(&page("requirements").blocks).unwrap().contains("needs input: the problem"));
}

#[test]
fn authored_intent_is_created_once_shown_as_authored_and_never_overwritten() {
    let work = tempfile::tempdir().unwrap();
    let repo = work.path().join("shop");
    copy_dir(&fixtures().join("polyglot-shop"), &repo);
    let out = work.path().join("book");
    generate(&repo, &out, &BookOptions::default()).unwrap();
    let path = out.join("authored.json");
    let template: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(template["operations"].get("api-gateway:POST /checkout").is_some(), "{template:#}");

    let edited = serde_json::json!({
        "business": { "problem": "Shoppers abandon carts when checkout is slow.", "goals": ["Checkout under 2 seconds"] },
        "operations": { "api-gateway:POST /checkout": { "name": "Place order", "actor": "Shopper", "purpose": "Turn a cart into a paid order" } }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&edited).unwrap()).unwrap();
    let second = generate(&repo, &out, &BookOptions::default()).unwrap();
    assert!(!second.written.contains(&"authored.json".to_string()));
    assert!(!second.removed.contains(&"authored.json".to_string()));
    assert_eq!(serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path).unwrap()).unwrap(), edited);

    let spec = std::fs::read_to_string(out.join("pages/07-functional-specification.md")).unwrap();
    assert!(spec.contains("Place order"), "{spec}");
    assert!(spec.contains("_authored_ Shopper"), "{spec}");
    let brd = std::fs::read_to_string(out.join("pages/08-business-requirements.md")).unwrap();
    assert!(brd.contains("_authored_ Shoppers abandon carts"), "{brd}");
    assert!(brd.contains("needs input: what is explicitly out of scope"), "{brd}");

    // A broken file is reported, not fatal, and not overwritten.
    std::fs::write(&path, "{ not json").unwrap();
    let third = generate(&repo, &out, &BookOptions::default()).unwrap();
    assert!(third.warnings.iter().any(|w| w.contains("authored.json")), "{:?}", third.warnings);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
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

#[test]
fn flows_page_documents_scheduled_jobs_and_handlers_with_background_requirements() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("flows"), out.path(), &BookOptions::default()).unwrap();
    let book = &planned.built.book;
    let page = |id: &str| book.pages.iter().find(|p| p.id == id).unwrap_or_else(|| panic!("page {id}"));
    let headings = |id: &str| -> Vec<String> {
        page(id)
            .blocks
            .iter()
            .filter_map(|b| if let Block::Heading { text, .. } = b { Some(text.clone()) } else { None })
            .collect()
    };
    let flows = headings("flows");
    for h in [
        "Request flows",
        "POST /orders",
        "Scheduled jobs",
        // The schedule reads its cron from configuration, so the heading shows the value.
        "expire · cron 0 0 3 * * * (stock.expiry.cron)",
        "reconcile · cron 0 0 * * * *",
        "Message & event handlers",
        "onOrder · Kafka topic order.created",
        "on · StockReserved",
        "onExpiry · StockExpired",
    ] {
        assert!(flows.contains(&h.to_string()), "flows page lacks `{h}`: {flows:#?}");
    }
    // Jobs past the figure budget are still listed, not dropped.
    let flows_text = serde_json::to_string(&page("flows").blocks).unwrap();
    assert!(flows.contains(&"Other scheduled jobs".to_string()), "{flows:#?}");
    for job in ["expireCarts", "collect"] {
        assert!(flows_text.contains(job), "`{job}` missing from the flows page");
    }
    let figures: Vec<&str> = planned.built.diagrams.iter().map(|d| d.id.as_str()).collect();
    for f in ["flow-orders-api-post-orders", "flow-inventory-schedule-expire", "flow-inventory-message-onorder"] {
        assert!(figures.contains(&f), "{f} not drawn: {figures:?}");
    }
    let orders = planned.built.diagrams.iter().find(|d| d.id == "flow-orders-api-post-orders").unwrap();
    let dashed = orders.ir.edges.iter().filter(|e| e.style == Some(nunki_ir::EdgeStyle::Dashed)).count();
    assert!(dashed >= 3, "asynchronous continuation is dashed: {:#?}", orders.ir.edges);

    let spec = headings("functional");
    assert!(spec.contains(&"Background processing".to_string()), "{spec:#?}");
    assert!(
        spec.iter().any(|h| h.starts_with("FR-") && h.contains("cron")),
        "a background requirement is identified by what triggers it: {spec:#?}"
    );
    let text = serde_json::to_string(&page("functional").blocks).unwrap();
    assert!(text.contains("cron 0 * * * *") && text.contains("Continues in"), "{text}");
}

#[test]
fn modular_monolith_container_page_documents_module_contracts_and_boundary_violations() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("real-world/spring-modulith"), out.path(), &BookOptions::default()).unwrap();
    let book = &planned.built.book;
    let page = book.pages.iter().find(|p| p.id == "containers/spring-modulith").expect("container page");
    let text = serde_json::to_string(&page.blocks).unwrap();
    for needle in [
        "Public API",
        "OrderLookup",
        "OrderCompleted",
        "Boundary violations",
        "2 boundary violations",
        "dependency not allowed",
        "uses internal type",
        "com.example.order.internal.OrderRepository",
    ] {
        assert!(text.contains(needle), "container page lacks {needle}");
    }
    // Small model: one data figure, no domain pages.
    assert!(book.pages.iter().all(|p| !p.id.starts_with("data/")));
    assert!(book.diagrams.contains_key("data-model"));
}

/// The manifest is read back from the output directory, and a book may be
/// generated for a repository the user does not control. Its recorded file list
/// drives the pruning pass, so a crafted entry must not reach a file outside the
/// book — by traversal, by an absolute path, or through a symlinked directory.
#[test]
fn a_crafted_manifest_cannot_delete_or_read_outside_the_book() {
    let repo = fixtures().join("go-mini");
    let out = tempfile::tempdir().unwrap();
    let opts = BookOptions::default();
    generate(&repo, out.path(), &opts).unwrap();

    // A bystander file beside the book, and one the book legitimately owns.
    let outside = out.path().parent().unwrap().join("outside-the-book.txt");
    std::fs::write(&outside, "not the book's to delete").unwrap();
    let stale = out.path().join("pages").join("99-stale.md");
    std::fs::write(&stale, "generated by an earlier run").unwrap();

    let manifest_path = out.path().join(nunki_book::MANIFEST);
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let files = manifest.get_mut("files").unwrap().as_object_mut().unwrap();
    files.insert("../outside-the-book.txt".into(), serde_json::json!("x"));
    files.insert(outside.display().to_string(), serde_json::json!("x"));
    files.insert("pages/../../outside-the-book.txt".into(), serde_json::json!("x"));
    // Read side: a curated-diagram entry that climbs out of the book.
    files.insert("diagrams/../../outside-the-book.ir.json".into(), serde_json::json!("x"));
    files.insert("pages/99-stale.md".into(), serde_json::json!("x"));
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

    let report = generate(&repo, out.path(), &opts).unwrap();

    assert!(outside.is_file(), "a manifest entry deleted a file outside the book");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "not the book's to delete",
        "the file outside the book was modified"
    );
    assert!(!stale.exists(), "a genuinely stale file inside the book should still be pruned");
    assert!(report.removed.iter().any(|r| r == "pages/99-stale.md"));
    assert!(!report.removed.iter().any(|r| r.contains("outside-the-book")), "removed: {:?}", report.removed);
}

/// The book must agree with itself about how much of it is verified. Figure
/// nodes carry citations too, and while they were registered after the evidence
/// page was built, that page reported a smaller total than the README and the
/// manifest — and a stale citation among them never reached the page's warning.
#[test]
fn the_evidence_page_counts_every_citation_the_book_makes() {
    for name in FIXTURES {
        let out = tempfile::tempdir().unwrap();
        let planned = plan(&fixtures().join(name), out.path(), &BookOptions::default()).unwrap();
        let book = &planned.built.book;

        assert_eq!(
            book.meta.evidence.total,
            book.cites.len(),
            "{name}: the recorded total is not the number of citations the book holds"
        );

        let page = book.pages.iter().find(|p| p.id == "evidence").expect("every book has an evidence page");
        let stat = page
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Stats { items } => items.iter().find(|s| s.label == "Citations"),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: the evidence page states a citation count"));
        assert_eq!(
            stat.value,
            book.meta.evidence.total.to_string(),
            "{name}: the evidence page and the book's own metadata disagree"
        );
    }
}

/// Paths, symbol names and doc comments come from the documented repository,
/// which is not necessarily code the reader trusts. A route path carrying
/// `](https://…)` used to close the link label the renderer had opened, putting
/// an attacker-chosen destination in the operations table of a published book.
#[test]
fn repository_content_cannot_inject_markdown_structure() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("cmd")).unwrap();
    std::fs::write(repo.join("go.mod"), "module x\n\ngo 1.22\n\nrequire github.com/go-chi/chi/v5 v5.0.12\n").unwrap();
    std::fs::write(
        repo.join("cmd/main.go"),
        r#"package main

import (
	"net/http"

	"github.com/go-chi/chi/v5"
)

func main() {
	r := chi.NewRouter()
	r.Get("/x](https://evil.example)", handle)
	http.ListenAndServe(":8080", r)
}

func handle(w http.ResponseWriter, r *http.Request) {}
"#,
    )
    .unwrap();

    let out = tempfile::tempdir().unwrap();
    let planned = plan(&repo, out.path(), &BookOptions::default()).unwrap();

    // The path must still be documented — escaping it, not dropping it.
    let api = planned.files.iter().find(|(p, _)| p.contains("06-api")).map(|(_, t)| t.clone()).unwrap_or_default();
    assert!(api.contains("evil.example"), "the route should still be documented, escaped");

    // …with its bracket escaped, so it cannot close the label the renderer
    // opened. Unescaped, `[/x](https:/evil.example)](…)` is a live link to a
    // destination the documented repository chose.
    for (path, text) in &planned.files {
        if !path.ends_with(".md") {
            continue;
        }
        assert!(
            !text.contains("x](https:/evil.example)"),
            "{path} lets the repository close a link label:\n{}",
            text.lines().filter(|l| l.contains("evil.example")).collect::<Vec<_>>().join("\n")
        );
    }
    assert!(api.contains("\\](https:/evil.example)"), "the bracket should be escaped, not removed");
}

/// Anchors are derived from human text, and `GET /users` and `GET /users/`
/// slug to the same thing. Two headings sharing one id send every link to the
/// first, so a reader following an operation's contract link lands on a
/// different operation — and a model whose anchor collided was dropped from
/// the page entirely while links kept pointing at it.
#[test]
fn operations_that_slug_alike_get_their_own_anchors() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("cmd")).unwrap();
    std::fs::write(repo.join("go.mod"), "module x\n\ngo 1.22\n\nrequire github.com/go-chi/chi/v5 v5.0.12\n").unwrap();
    std::fs::write(
        repo.join("cmd/main.go"),
        r#"package main

import (
	"net/http"

	"github.com/go-chi/chi/v5"
)

func main() {
	r := chi.NewRouter()
	r.Get("/user-profile", list)
	r.Get("/user_profile", listSlash)
	http.ListenAndServe(":8080", r)
}

func list(w http.ResponseWriter, r *http.Request)      {}
func listSlash(w http.ResponseWriter, r *http.Request) {}
"#,
    )
    .unwrap();

    let out = tempfile::tempdir().unwrap();
    let planned = plan(&repo, out.path(), &BookOptions::default()).unwrap();
    let book = &planned.built.book;

    let mut ids: Vec<&str> = Vec::new();
    for page in &book.pages {
        for b in &page.blocks {
            if let Block::Heading { id, .. } = b {
                ids.push(id);
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let duplicates: Vec<&&str> = ids.iter().filter(|id| !seen.insert(**id)).collect();
    assert!(duplicates.is_empty(), "two headings share an anchor, so links reach the wrong one: {duplicates:?}");

    // Both routes are still documented — disambiguated, not dropped.
    assert_eq!(
        ids.iter().filter(|id| id.starts_with("op-get-user-profile")).count(),
        2,
        "both operations should have a heading: {ids:?}"
    );
}

/// What the book could not read belongs on its first page.
///
/// A reader cannot otherwise tell a book about a whole system from a book about
/// the quarter of it that happened to be in a language nunki parses. Measured
/// on immich this is the difference between a confident survey and an honest
/// one: 765 of 1918 source files read, its Dart app and Svelte UI unseen.
#[test]
fn the_overview_says_how_much_of_the_source_was_read() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("real-world/mostly-unread"), out.path(), &BookOptions::default()).unwrap();
    let overview = planned.built.book.pages.iter().find(|p| p.id == "overview").expect("overview page");

    let callout = overview
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Callout { tone, title, inl } if title.contains("source was read") => {
                let text: String = inl
                    .iter()
                    .filter_map(|i| match i {
                        Inline::Text { v } => Some(v.clone()),
                        _ => None,
                    })
                    .collect();
                Some((tone.clone(), title.clone(), text))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("a coverage callout on the overview: {:?}", overview.blocks));

    let (tone, title, text) = callout;
    assert_eq!(title, "24% of the source was read");
    // Under four fifths the gap is a fact about the book, not a footnote.
    assert_eq!(tone, "warning", "a quarter read is a warning, not a note");
    assert!(text.contains("5 of 21 source files"), "{text}");
    assert!(text.contains("12 C++, 4 Dart"), "names them, most numerous first: {text}");
    assert!(text.contains("absent here rather than absent from the code"), "{text}");

    // A fully readable repository says nothing, rather than boasting 100%.
    let out2 = tempfile::tempdir().unwrap();
    let clean = plan(&fixtures().join("polyglot-shop"), out2.path(), &BookOptions::default()).unwrap();
    let clean_overview = clean.built.book.pages.iter().find(|p| p.id == "overview").unwrap();
    assert!(
        !clean_overview
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Callout { title, .. } if title.contains("source was read"))),
        "no coverage callout when everything was read"
    );
}

/// The behaviour model has to leave as data, not only as prose. A Markdown
/// table cannot be compared against a specification someone else wrote, handed
/// to an agent, or read by anything that wants more than page text.
///
/// What this guards is not that the file exists but that it agrees with the
/// pages built beside it: the same requirement identifiers, the same rules,
/// and evidence a reader can follow back rather than trust.
#[test]
fn behaviour_leaves_as_data_and_agrees_with_the_prose() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("polyglot-shop"), out.path(), &BookOptions::default()).unwrap();

    let text = planned.files.get("behaviour.json").expect("written beside the book");
    let v: serde_json::Value = serde_json::from_str(text).unwrap();

    // Provenance, so a consumer knows what it is reading and can re-verify.
    let p = &v["provenance"];
    assert!(p["generator"].as_str().is_some_and(|g| g.starts_with("nunki")), "{p}");
    assert_eq!(p["citationsVerified"], p["citationsTotal"], "the book verified everything it cites");
    assert!(p["filesRead"].as_u64().unwrap_or(0) > 0);

    let reqs = v["requirements"].as_array().unwrap();
    let rules = v["rules"].as_array().unwrap();
    assert!(reqs.len() >= 5, "{} requirements", reqs.len());
    assert!(!rules.is_empty());

    // Every requirement carries the identifier it is cited by, the operation it
    // came from, and evidence.
    for r in reqs {
        let id = r["id"].as_str().unwrap();
        assert!(id.starts_with("FR-"), "{id}");
        assert!(r["operation"].as_str().is_some_and(|o| o.contains(':')), "{r}");
        assert!(r["evidence"]["filePath"].as_str().is_some_and(|f| !f.is_empty()), "{r}");
        assert!(r["evidence"]["startLine"].as_u64().unwrap_or(0) > 0, "{r}");
    }

    // The identifiers are the ones the functional page prints. A model that
    // disagreed with the prose beside it would be worse than no model.
    let page =
        serde_json::to_string(&planned.built.book.pages.iter().find(|p| p.id == "functional").unwrap().blocks).unwrap();
    for r in reqs {
        let id = r["id"].as_str().unwrap();
        assert!(page.contains(id), "{id} is in the model but not on the page");
    }
    for r in rules {
        let id = r["id"].as_str().unwrap();
        assert!(page.contains(id), "{id} is in the model but not on the page");
    }

    // Rules name requirements, not raw operations, so a consumer has one kind
    // of key to understand.
    let ids: std::collections::BTreeSet<&str> = reqs.iter().map(|r| r["id"].as_str().unwrap()).collect();
    let mut linked = 0;
    for rule in rules {
        for req in rule["requirements"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let r = req.as_str().unwrap();
            assert!(ids.contains(r), "rule {} names {r}, which is not a requirement", rule["id"]);
            linked += 1;
        }
    }
    assert!(linked > 0, "rules that constrain nothing would mean the link was never built");

    // And a requirement's rule list is the mirror of that.
    for r in reqs {
        for rule_id in r["rules"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let rule = rules.iter().find(|x| x["id"] == *rule_id).expect("rule exists");
            assert!(
                rule["requirements"].as_array().unwrap().iter().any(|x| x == &r["id"]),
                "{} lists {rule_id}, which does not list it back",
                r["id"]
            );
        }
    }
}

/// The OpenSpec renderer, against the rules their validator enforces. CI runs
/// the real `openspec validate --strict`; this is the same contract stated
/// locally, so a change that breaks it fails before it reaches a runner that
/// happens to have node.
#[test]
fn openspec_output_follows_the_rules_their_validator_enforces() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("polyglot-shop"), out.path(), &BookOptions::default()).unwrap();
    let files = nunki_book::openspec::render(&planned.built.behaviour, &|_| None);
    assert!(files.len() >= 3, "one capability per service: {:?}", files.keys().collect::<Vec<_>>());

    for (path, text) in &files {
        assert!(path.starts_with("openspec/specs/") && path.ends_with("/spec.md"), "{path}");
        // Required sections.
        assert!(text.contains("\n## Purpose\n"), "{path} has no Purpose");
        assert!(text.contains("\n## Requirements\n"), "{path} has no Requirements");

        let reqs: Vec<&str> = text.lines().filter(|l| l.starts_with("### Requirement: ")).collect();
        assert!(!reqs.is_empty(), "{path} declares no requirement");

        // The header text is the identity in this format, so duplicates inside
        // one file are a validation error, not a cosmetic problem.
        let mut seen = BTreeSet::new();
        for r in &reqs {
            assert!(seen.insert(r.trim()), "{path} repeats {r}, which their validator rejects");
        }

        // Every requirement needs narrative containing SHALL before its first
        // scenario; an empty body is an error and a missing modal fails strict.
        for chunk in text.split("### Requirement: ").skip(1) {
            let name = chunk.lines().next().unwrap_or("");
            let body = chunk.split("#### Scenario:").next().unwrap_or("");
            assert!(
                body.contains("SHALL") || body.contains("MUST"),
                "{path}: requirement {name:?} has no SHALL, which strict mode fails on"
            );
            assert!(
                body.lines().filter(|l| !l.starts_with("<!--") && !l.trim().is_empty()).count() >= 2,
                "{path}: requirement {name:?} has an empty body"
            );
        }

        // Scenarios are level four with bolded keywords; bulleted WHEN/THEN
        // lines outside a Scenario header are warned about.
        for line in text.lines().filter(|l| l.starts_with("- **")) {
            assert!(
                line.starts_with("- **WHEN**") || line.starts_with("- **THEN**") || line.starts_with("- **AND**"),
                "{path}: {line:?} is not one of their keywords"
            );
        }

        // Our identifier and evidence travel in a comment, because their format
        // has no field for either and the heading is theirs.
        assert!(text.contains("<!-- nunki:FR-"), "{path} carries no identifier back to the book");
        assert!(text.contains(" evidence:"), "{path} carries no evidence");
    }

    // Nothing is claimed that was not measured: every status in a scenario is
    // one the model recorded.
    let statuses: BTreeSet<String> = planned
        .built
        .behaviour
        .requirements
        .iter()
        .flat_map(|r| r.success_status.into_iter().chain(r.error_statuses.iter().copied()))
        .map(|c| c.to_string())
        .collect();
    for text in files.values() {
        for line in text.lines().filter(|l| l.starts_with("#### Scenario: Request fails with ")) {
            let code = line.rsplit(' ').next().unwrap();
            assert!(statuses.contains(code), "{line:?} invents a status the code never returns");
        }
    }
}

/// A service calling out to something this repository does not contain.
///
/// These calls were extracted, stored in the model, and rendered nowhere:
/// every use of `client_calls` in the book asks for the ones that resolved to
/// an operation, so an outbound dependency on another repository produced a
/// book that said nothing about it. Documentation that is confidently
/// incomplete is the failure this project exists to prevent, and it was
/// happening in the middle of its own output.
#[test]
fn outbound_calls_the_book_cannot_attribute_are_still_documented() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("cross-repo"), out.path(), &BookOptions::default()).unwrap();
    let page = planned.built.book.pages.iter().find(|p| p.id == "evidence").expect("evidence page");
    let text = serde_json::to_string(&page.blocks).unwrap();

    assert!(text.contains("Calls that leave what is documented"), "{text}");
    assert!(text.contains("POST /v1/notifications"), "the call itself: {text}");

    // The host, which is in neither line on its own: it is declared as a const
    // with a literal default and used through a template.
    assert!(text.contains("notifications"), "the host it is aimed at: {text}");

    assert!(text.contains("notifyShipped"), "and the function responsible: {text}");

    // Cited at the line that makes the call. The page references a citation by
    // id, so the claim is only real if that id resolves to the right file.
    let cited: Vec<&str> = planned
        .built
        .book
        .cites
        .values()
        .filter(|c| c.file.ends_with("clients/notify.ts"))
        .map(|c| c.file.as_str())
        .collect();
    assert!(!cited.is_empty(), "no citation lands in the file that makes the call");

    // The operation that does resolve is not dragged in here.
    let model = &planned.built.report.api.as_ref().unwrap();
    let unresolved = model.client_calls.iter().filter(|c| c.operation.is_none()).count();
    assert_eq!(unresolved, 1, "{:?}", model.client_calls);
    assert_eq!(
        model.client_calls.iter().filter(|c| c.target_host.is_some()).count(),
        1,
        "the host is kept on the model, not only rendered"
    );
}

/// A repository whose calls all resolve says nothing about calls that leave,
/// rather than printing an empty section.
#[test]
fn a_repository_with_no_outbound_calls_says_nothing_about_them() {
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("polyglot-shop"), out.path(), &BookOptions::default()).unwrap();
    let page = planned.built.book.pages.iter().find(|p| p.id == "evidence").unwrap();
    let text = serde_json::to_string(&page.blocks).unwrap();
    assert!(!text.contains("Calls that leave"), "an empty section is noise: {text}");
}

/// The business-requirements page says where the system's description came
/// from, and that claim has to be true.
///
/// It badged every description "from README" — including the ones that fell back
/// to a package manifest when no README sentence fitted. nunki's own book
/// introduced itself with `nunki-analyzer`'s manifest line, labelled as coming
/// from a README that says something else, on the page whose entire subject is
/// which statements are evidenced and which are not.
#[test]
fn a_description_is_labelled_with_where_it_actually_came_from() {
    for name in ["polyglot-shop", "go-mini", "cli-tool"] {
        let out = tempfile::tempdir().unwrap();
        let planned = plan(&fixtures().join(name), out.path(), &BookOptions::default()).unwrap();
        let Some(page) = planned.built.book.pages.iter().find(|p| p.id == "requirements") else { continue };
        let text = serde_json::to_string(&page.blocks).unwrap();

        if text.contains("from README") {
            // A claim about provenance needs the same evidence as any other: the
            // page must cite the README it says the words came from.
            let cited = planned.built.book.cites.values().any(|c| c.file.to_uppercase().contains("README"));
            assert!(cited, "{name}: says `from README` and cites no README");
        }
        // Whichever label it used, the fallback must not borrow the other's name.
        let repo_has_readme = fixtures().join(name).join("README.md").is_file();
        if !repo_has_readme {
            assert!(!text.contains("from README"), "{name} has no README, so nothing can come from one");
        }
    }
}
