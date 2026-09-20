//! Integration: business domains of the data model and module boundaries of
//! modular monoliths, on fixture repositories. Evidence lines are read back.

use std::path::{Path, PathBuf};

use nunki_analyzer::data::DataModel;
use nunki_analyzer::domains::{domains, draft_domain_irs, draft_domain_overview_ir};
use nunki_analyzer::scan::ComponentView;
use nunki_analyzer::{scan, Depth, DraftOptions, EvidenceRef, ScanOptions};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

/// The cited lines (start..=end), joined.
fn line_of(root: &Path, e: &EvidenceRef) -> String {
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(e.start_line >= 1 && e.end_line as usize <= lines.len(), "{e:?} out of range");
    lines[e.start_line as usize - 1..e.end_line as usize].join("\n")
}

fn opts() -> DraftOptions {
    DraftOptions { generated_at: Some("2026-01-01T00:00:00Z".into()), ..Default::default() }
}

#[test]
fn every_entity_gets_a_domain_and_small_models_stay_whole() {
    for name in [
        "polyglot-shop",
        "orm-models/jpa-spring",
        "orm-models/mongo-spring",
        "orm-models/liquibase",
        "orm-models/node-app",
        "orm-models/py-app",
        "orm-models/go-app",
        "orm-models/rust-app",
        "orm-models/quarkus-panache",
        "real-world/spring-modulith",
    ] {
        let root = fixture(name);
        let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
        let data = r.data.as_ref().unwrap();
        for e in &data.entities {
            assert!(e.domain.as_deref().is_some_and(|d| !d.is_empty()), "{name}: {} has no domain", e.table);
        }
        // ≤ 12 entities read as one figure: no domain split, no overview.
        if data.entities.len() <= nunki_analyzer::views::MAX_ENTITIES {
            assert!(domains(data).is_empty(), "{name}");
            assert!(draft_domain_irs(&r, data, &opts()).is_empty(), "{name}");
            assert!(draft_domain_overview_ir(&r, data, &opts()).is_none(), "{name}");
        }
    }
}

#[test]
fn java_entities_take_the_module_package_even_when_internal() {
    let root = fixture("real-world/spring-modulith");
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    let data = r.data.as_ref().unwrap();
    let domain = |t: &str| data.entities.iter().find(|e| e.table == t).and_then(|e| e.domain.clone());
    assert_eq!(domain("orders").as_deref(), Some("order"));
    assert_eq!(domain("stock_items").as_deref(), Some("inventory"), "com.example.inventory.internal → inventory");
}

/// A model large enough to split: the jpa-spring and orm fixtures merged with
/// synthetic domains, drafted and validated.
#[test]
fn large_models_draft_valid_domain_figures_with_cross_domain_stubs_and_an_overview() {
    let root = fixture("real-world/spring-modulith");
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    let base = r.data.clone().unwrap();
    let template = base.entities.iter().find(|e| e.table == "orders").unwrap().clone();
    let mut data = DataModel { entities: vec![], state_machines: vec![] };
    for (domain, count) in [("billing", 6), ("catalog", 5), ("shipping", 4)] {
        for i in 0..count {
            let mut e = template.clone();
            e.table = format!("{domain}_{i}");
            e.id = format!("entity:{}", e.table);
            e.name = e.table.clone();
            e.domain = Some(domain.into());
            e.relations.clear();
            if i > 0 {
                e.relations.push(nunki_analyzer::data::Relation {
                    kind: "many-to-one".into(),
                    target: format!("entity:{domain}_0"),
                    via: format!("{domain}_0_id"),
                    evidence: e.evidence.clone(),
                });
            }
            if domain == "shipping" && i == 1 {
                e.relations.push(nunki_analyzer::data::Relation {
                    kind: "many-to-one".into(),
                    target: "entity:billing_0".into(),
                    via: "invoice_id".into(),
                    evidence: e.evidence.clone(),
                });
            }
            data.entities.push(e);
        }
    }
    let doms = domains(&data);
    assert_eq!(doms.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["billing", "catalog", "shipping"]);
    let validate = |ir: &nunki_ir::DiagramIR| {
        let v = nunki_validator::validate(
            ir,
            &nunki_validator::ValidateOptions { repo_root: Some(root.clone()), ..Default::default() },
        );
        assert!(v.valid, "{}: {:#?}", ir.title, v.diagnostics);
    };
    let figures = draft_domain_irs(&r, &data, &opts());
    assert_eq!(figures.len(), 3);
    for (d, draft) in &figures {
        validate(&draft.ir);
        assert!(
            draft.ir.nodes.iter().filter(|n| n.container_id.as_deref() != Some("other-domains")).count()
                == d.entities.len()
        );
    }
    let (_, shipping) = figures.iter().find(|(d, _)| d.name == "shipping").unwrap();
    let stub = shipping.ir.nodes.iter().find(|n| n.id == "ext_billing_0").expect("stub for billing_0");
    assert_eq!(stub.container_id.as_deref(), Some("other-domains"));
    assert_eq!(stub.subtitle.as_deref(), Some("in Billing"));
    assert!(shipping.ir.edges.iter().any(|e| e.source == "ext_billing_0" && e.target == "shipping_1"));

    let overview = draft_domain_overview_ir(&r, &data, &opts()).expect("overview");
    validate(&overview.ir);
    let edge =
        overview.ir.edges.iter().find(|e| e.source == "domain-shipping" && e.target == "domain-billing").unwrap();
    assert_eq!(edge.label.as_deref(), Some("1 reference"));
}

fn modules_view(root: &Path) -> ComponentView {
    scan(root, &ScanOptions { depth: Depth::Component, ..Default::default() }).unwrap().components.unwrap()
}

#[test]
fn modulith_modules_expose_api_events_internals_and_violations() {
    let root = fixture("real-world/spring-modulith");
    let view = modules_view(&root);
    assert!(view.boundaries_declared);
    let module = |id: &str| view.modules.iter().find(|m| m.id == format!("spring-modulith.{id}")).unwrap();

    let order = module("order");
    let api: Vec<&str> = order.public_types.iter().map(|t| t.name.as_str()).collect();
    assert!(api.contains(&"OrderManagement") && api.contains(&"OrderCompleted"), "{api:?}");
    assert!(api.contains(&"OrderLookup"), "named interface `spi` is public: {api:?}");
    assert!(!api.contains(&"OrderRepository"), "internal package isn't public: {api:?}");
    assert_eq!(order.publishes, ["OrderCompleted"]);
    assert_eq!(order.internal_packages, ["com.example.order.internal"]);
    for t in &order.public_types {
        assert!(line_of(&root, &t.evidence).contains(&t.name), "{t:?}");
    }

    let inventory = module("inventory");
    assert_eq!(inventory.consumes, ["OrderCompleted"]);
    assert_eq!(inventory.internal_packages, ["com.example.inventory.internal"]);
    assert!(module("shipping").publishes.is_empty());

    let v: Vec<(&str, &str, &str, &str)> = view
        .violations
        .iter()
        .map(|v| (v.from.as_str(), v.to.as_str(), v.kind.as_str(), v.specifier.as_str()))
        .collect();
    assert_eq!(
        v,
        [
            (
                "spring-modulith.shipping",
                "spring-modulith.inventory",
                "not-allowed",
                "com.example.inventory.StockLevel"
            ),
            (
                "spring-modulith.shipping",
                "spring-modulith.order",
                "internal",
                "com.example.order.internal.OrderRepository"
            ),
        ],
        "shipping may use order::spi only; importing order.internal and inventory breaks its boundary"
    );
    for x in &view.violations {
        assert!(line_of(&root, &x.evidence).contains(&x.specifier), "{x:?}");
    }
}

#[test]
fn package_by_layer_apps_report_no_false_boundary_violations() {
    // spring-cloud's accounts service: web/, client/, domain/ packages, no module declarations.
    let root = fixture("real-world/spring-cloud");
    let r = scan(&root, &ScanOptions { all_components: true, ..Default::default() }).unwrap();
    for view in &r.component_views {
        assert!(!view.boundaries_declared, "{}", view.unit);
        assert!(view.violations.is_empty(), "{}: {:?}", view.unit, view.violations);
    }
}

/// A domain no foreign key reaches is still part of the model: the overview
/// reaches it through the services that read and write it, and folds the
/// smallest domains into one card rather than dropping them.
#[test]
fn the_overview_accounts_for_every_domain() {
    let root = fixture("real-world/spring-modulith");
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    let base = r.data.clone().unwrap();
    let template = base.entities.iter().find(|e| e.table == "orders").unwrap().clone();
    let unit = r.containers.first().map(|c| c.id.clone()).unwrap();
    let mut data = DataModel { entities: vec![], state_machines: vec![] };
    for d in 0..20 {
        for i in 0..2 {
            let mut e = template.clone();
            e.table = format!("d{d}_t{i}");
            e.id = format!("entity:{}", e.table);
            e.name = e.table.clone();
            e.domain = Some(format!("domain{d:02}"));
            e.relations.clear();
            // Nothing references these: only the code that writes them connects them.
            e.writes = vec![nunki_analyzer::data::Access {
                unit: unit.clone(),
                symbol: Some("save".into()),
                evidence: e.evidence.clone(),
            }];
            e.reads.clear();
            data.entities.push(e);
        }
    }
    let names: Vec<String> = domains(&data).iter().map(|d| d.name.clone()).collect();
    assert_eq!(names.len(), 20);

    let overview = draft_domain_overview_ir(&r, &data, &opts()).expect("overview");
    let v = nunki_validator::validate(
        &overview.ir,
        &nunki_validator::ValidateOptions { repo_root: Some(root.clone()), ..Default::default() },
    );
    assert!(v.valid, "{:#?}", v.diagnostics);
    assert!(
        nunki_ir::visual_density(overview.ir.nodes.len(), overview.ir.edges.len()) <= 0.40,
        "{} nodes {} edges",
        overview.ir.nodes.len(),
        overview.ir.edges.len()
    );

    // Every domain is either its own card or named on the card that groups it.
    let grouped = overview.ir.nodes.iter().find(|n| n.label.starts_with("Other domains"));
    let listed = grouped.and_then(|n| n.tech_stack.clone()).unwrap_or_default();
    let note = overview.notes.join(" ");
    for name in &names {
        let drawn = overview.ir.nodes.iter().any(|n| n.id == format!("domain-{name}"));
        assert!(drawn || listed.contains(name.as_str()) || note.contains(name.as_str()), "{name} unaccounted for");
    }
    assert!(grouped.is_some(), "the smallest domains fold into one card: {:?}", overview.notes);
    // Services connect the domains that no foreign key reaches.
    assert!(overview.ir.edges.iter().any(|e| e.source == unit), "{:#?}", overview.ir.edges);
}
