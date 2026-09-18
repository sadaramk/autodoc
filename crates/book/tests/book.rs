//! Integration: book structure, determinism, link integrity and citations on
//! every fixture repository.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use autodoc_book::model::{Block, Inline};
use autodoc_book::{generate, plan, BookOptions};

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
    "flows",
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
        let report = autodoc_validator::validate(&d.ir, &autodoc_validator::ValidateOptions::default());
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
    assert!(fr.iter().any(|h| h.starts_with("FR-001")), "{fr:?}");
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
    let dashed = orders.ir.edges.iter().filter(|e| e.style == Some(autodoc_ir::EdgeStyle::Dashed)).count();
    assert!(dashed >= 3, "asynchronous continuation is dashed: {:#?}", orders.ir.edges);

    let spec = headings("functional");
    assert!(spec.contains(&"Background processing".to_string()), "{spec:#?}");
    assert!(spec.iter().any(|h| h.starts_with("FR-002 · ")), "background FRs continue the numbering: {spec:#?}");
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
