//! Regression fixtures reproducing the shapes of public repositories the
//! analyzer was validated against (compose in subdirectories, shared env
//! files, unparsed languages, large Rust workspaces, uv workspaces). The code
//! is synthetic; only the structure mirrors the originals.

use std::path::{Path, PathBuf};

use autodoc_analyzer::lang::Language;
use autodoc_analyzer::scan::{EvidenceRef, RelationSource, Relationship};
use autodoc_analyzer::{draft_ir, scan, Depth, DraftOptions, ScanOptions, ScanReport, UnitKind};
use autodoc_ir::EdgeType;

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/real-world")).join(name)
}

fn scan_container(root: &Path) -> ScanReport {
    scan(root, &ScanOptions::default()).unwrap()
}

fn assert_evidence_real(root: &Path, e: &EvidenceRef) {
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        e.start_line >= 1 && e.start_line <= e.end_line && e.end_line as usize <= lines.len(),
        "{e:?} out of range ({} lines)",
        lines.len()
    );
    if let Some(sym) = &e.symbol_name {
        let window = &lines[e.start_line as usize - 1..e.end_line as usize];
        assert!(window.iter().any(|l| l.contains(sym.as_str()) || l.contains("__name__")), "{sym} not within {e:?}");
    }
}

fn unit<'a>(r: &'a ScanReport, id: &str) -> &'a autodoc_analyzer::scan::UnitSummary {
    r.containers.iter().find(|u| u.id == id).unwrap_or_else(|| {
        panic!("unit `{id}` missing; have {:?}", r.containers.iter().map(|u| &u.id).collect::<Vec<_>>())
    })
}

fn rel<'a>(r: &'a ScanReport, s: &str, t: &str) -> &'a Relationship {
    r.relationships.iter().find(|x| x.source == s && x.target == t).unwrap_or_else(|| {
        panic!(
            "{s} → {t} missing; have {:?}",
            r.relationships.iter().map(|x| format!("{} → {}", x.source, x.target)).collect::<Vec<_>>()
        )
    })
}

fn has_rel(r: &ScanReport, s: &str, t: &str) -> bool {
    r.relationships.iter().any(|x| x.source == s && x.target == t)
}

fn assert_all_evidence_real(root: &Path, r: &ScanReport) {
    for e in r.relationships.iter().flat_map(|x| &x.evidence).chain(r.evidence_map.values()) {
        assert_evidence_real(root, e);
    }
    for u in &r.containers {
        for e in &u.entry_points {
            assert_evidence_real(root, e);
        }
    }
}

fn assert_draft_valid(root: &Path, r: &ScanReport) -> autodoc_ir::DiagramIR {
    let d = draft_ir(r, &DraftOptions { generated_at: Some("2026-01-01T00:00:00Z".into()), ..Default::default() });
    let v = autodoc_validator::validate(
        &d.ir,
        &autodoc_validator::ValidateOptions { repo_root: Some(root.to_path_buf()), ..Default::default() },
    );
    assert!(v.valid, "{}: {:#?}\nnotes: {:#?}", root.display(), v.diagnostics, d.notes);
    d.ir
}

#[test]
fn compose_in_subdirectory_with_dockerfile_contexts_and_literal_urls() {
    let root = fixture("compose-subdir");
    let r = scan_container(&root);
    assert_eq!(unit(&r, "server").kind, UnitKind::HttpService);
    let ml = unit(&r, "ml");
    assert_eq!(ml.kind, UnitKind::HttpService);
    assert_eq!(ml.entry_points[0].file_path, "ml/shop_ml/main.py", "FastAPI app beats the export script");
    assert_eq!(unit(&r, "web").kind, UnitKind::WebClient);
    assert_eq!(unit(&r, "sdk").kind, UnitKind::Library);
    assert!(!r.containers.iter().any(|u| u.id == "docs" || u.root.starts_with("docs")), "docs/ is excluded");

    let server_ml = rel(&r, "server", "ml");
    assert_eq!(server_ml.label, "HTTP");
    assert!(server_ml.evidence.iter().any(|e| e.file_path == "server/src/config.ts"), "{server_ml:?}");
    assert_eq!(rel(&r, "web", "server").label, "HTTP");
    let web_sdk = rel(&r, "web", "sdk");
    assert_eq!(web_sdk.label, "uses");
    assert!(web_sdk.sources.contains(&RelationSource::Code));
    assert_eq!(rel(&r, "server", "postgres").edge_type, EdgeType::Write);
    rel(&r, "server", "redis");
    let pg = r.infrastructure.iter().find(|i| i.id == "postgres").unwrap();
    assert_eq!(pg.compose_service.as_deref(), Some("database"));

    assert_all_evidence_real(&root, &r);
    assert_draft_valid(&root, &r);
}

#[test]
fn shared_env_file_edges_need_code_confirmation_through_libraries() {
    let root = fixture("grpc-env-file");
    let r = scan_container(&root);
    let grpc = rel(&r, "trainings", "trainer");
    assert_eq!(grpc.label, "gRPC");
    assert!(grpc.sources.contains(&RelationSource::Code), "{grpc:?}");
    assert!(grpc.evidence.iter().any(|e| e.file_path == "internal/trainings/main.go"), "{grpc:?}");
    assert!(!has_rel(&r, "trainer", "trainer"));
    assert!(r.relationships.iter().all(|x| x.source != x.target));
    assert_eq!(rel(&r, "trainings", "common").label, "uses");
    assert_eq!(rel(&r, "trainer", "common").label, "uses");
    assert_eq!(unit(&r, "common").kind, UnitKind::Library);

    assert_all_evidence_real(&root, &r);
    assert_draft_valid(&root, &r);
}

#[test]
fn unparsed_language_services_still_appear_and_generic_gets_are_not_reads() {
    let root = fixture("unparsed-worker");
    let r = scan_container(&root);
    let worker = unit(&r, "worker");
    assert_eq!(worker.language, Language::CSharp);
    assert_eq!(worker.kind, UnitKind::Worker);
    assert_eq!(worker.entry_points[0].file_path, "worker/Dockerfile");
    rel(&r, "worker", "redis");
    rel(&r, "worker", "postgres");
    let vote = rel(&r, "vote", "redis");
    assert_eq!(vote.edge_type, EdgeType::Write);
    assert_eq!(vote.label, "writes");

    assert_all_evidence_real(&root, &r);
    assert_draft_valid(&root, &r);
}

#[test]
fn large_rust_workspace_groups_libraries_and_keeps_the_server() {
    let root = fixture("rust-workspace");
    let r = scan_container(&root);
    assert_eq!(unit(&r, "server").kind, UnitKind::HttpService);
    assert_eq!(unit(&r, "utils").kind, UnitKind::Library, "lib + helper bin linked by others is a library");
    assert_eq!(unit(&r, "routes").kind, UnitKind::Library, "framework dependency without an entry point");
    assert!(!has_rel(&r, "routes", "utils"), "dev-dependencies are not architecture");
    assert_all_evidence_real(&root, &r);

    let ir = assert_draft_valid(&root, &r);
    let focal: Vec<&str> = ir.nodes.iter().filter(|n| n.is_key_focal_point).map(|n| n.id.as_str()).collect();
    assert_eq!(focal, vec!["server"]);
    assert!(ir.edges.iter().any(|e| e.source == "server" || e.target == "server"), "server must not be orphaned");
    assert!(
        ir.nodes.iter().any(|n| n.id.starts_with("libs-") && n.label.contains("(6 crates)")),
        "{:?}",
        ir.nodes.iter().map(|n| (&n.id, &n.label)).collect::<Vec<_>>()
    );
    for e in ir.edges.iter().filter(|e| e.primary()) {
        let target = ir.node(&e.target).unwrap();
        assert_ne!(target.container_id.as_deref(), Some("libraries"), "primary path entered a library: {e:?}");
    }
}

#[test]
fn uv_workspace_orm_migrations_and_readme_summary() {
    let root = fixture("uv-workspace");
    let r = scan_container(&root);
    assert!(!r.containers.iter().any(|u| u.root.is_empty()), "uv workspace root is not a unit");
    assert!(!r.containers.iter().any(|u| u.id == "scripts" || u.root.starts_with("scripts")));
    assert_eq!(unit(&r, "backend").entry_points[0].file_path, "backend/app/main.py");

    let db = rel(&r, "backend", "postgres");
    assert_eq!(db.label, "queries", "{db:?}");
    assert!(db.sources.contains(&RelationSource::Code));
    assert!(!r.relationships.iter().any(|x| x.label.contains("item")), "migrations are not runtime access");
    assert_eq!(rel(&r, "frontend", "backend").label, "HTTP");
    assert!(!has_rel(&r, "backend", "emails"), "npm `emails` is not the Python `emails` dependency");
    assert_eq!(r.system.description.as_deref(), Some("Acme Template is a full stack starter for FastAPI and React."));

    assert_all_evidence_real(&root, &r);
    assert_draft_valid(&root, &r);
}

#[test]
fn every_shape_renders_all_depths() {
    for name in ["compose-subdir", "grpc-env-file", "unparsed-worker", "rust-workspace", "uv-workspace"] {
        let root = fixture(name);
        for depth in [Depth::System, Depth::Container, Depth::Component] {
            let r = scan(&root, &ScanOptions { depth, ..Default::default() }).unwrap();
            let d = draft_ir(&r, &DraftOptions::default());
            assert!(!d.ir.nodes.is_empty(), "{name}/{depth:?} drafted nothing");
        }
    }
}

#[test]
fn spring_cloud_services_config_routes_feign_and_messaging() {
    let root = fixture("spring-cloud");
    let r = scan_container(&root);
    let ids: Vec<&str> = r.containers.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["accounts", "common", "config", "gateway", "stats"], "maven aggregator isn't a unit");
    assert_eq!(
        unit(&r, "common").kind,
        UnitKind::Library,
        "a JVM module with a listener but no application is a library"
    );
    let uses = rel(&r, "stats", "common");
    assert!(uses.sources.contains(&RelationSource::Code), "artifactId dependency backed by an import: {uses:?}");
    for id in ["accounts", "gateway", "stats", "config"] {
        let u = unit(&r, id);
        assert_eq!((u.kind, u.language), (UnitKind::HttpService, Language::Java), "{id}");
        assert!(
            u.entry_points[0].note.as_deref().unwrap().contains("@SpringBootApplication"),
            "{id}: {:?}",
            u.entry_points
        );
    }
    assert_eq!(unit(&r, "gateway").frameworks[0], "Zuul");
    assert_eq!(unit(&r, "config").frameworks[0], "Spring Cloud Config Server");
    assert!(
        unit(&r, "gateway").tech_stack.starts_with("Java"),
        "bundled app.js under resources is not the gateway's language"
    );

    // Gateway routes come from the config server's shared/gateway.yml, by service id and by URL host.
    let route = rel(&r, "gateway", "accounts");
    assert_eq!(
        (route.label.as_str(), route.sources.as_slice()),
        ("routes /accounts/**", &[RelationSource::Config][..])
    );
    assert_eq!(route.evidence[0].file_path, "config/src/main/resources/shared/gateway.yml");
    assert_eq!(rel(&r, "gateway", "stats").label, "routes /statistics/**");

    // @FeignClient(name = "statistics-service") resolves through spring.application.name.
    let feign = rel(&r, "accounts", "stats");
    assert!(feign.sources.contains(&RelationSource::Code));
    assert_eq!(feign.evidence[0].file_path, "accounts/src/main/java/com/acme/accounts/client/StatsClient.java");

    let mongo = r.infrastructure.iter().find(|i| i.id == "mongodb").expect("mongodb");
    assert!(mongo.used_by.contains(&"accounts".to_string()));
    let rabbit = r.infrastructure.iter().find(|i| i.id == "rabbitmq").expect("rabbitmq");
    assert!(rabbit.used_by.contains(&"accounts".to_string()) && rabbit.used_by.contains(&"stats".to_string()));
    let consumer = rel(&r, "rabbitmq", "stats");
    assert!(
        consumer.evidence.iter().any(|e| e.file_path.ends_with("stats/example/AccountEvents.java")),
        "{consumer:?}"
    );
    assert!(rel(&r, "accounts", "rabbitmq").evidence.iter().any(|e| e.file_path.ends_with("AccountController.java")));
    assert_all_evidence_real(&root, &r);

    let ir = assert_draft_valid(&root, &r);
    let focal: Vec<&str> = ir.nodes.iter().filter(|n| n.is_key_focal_point).map(|n| n.id.as_str()).collect();
    assert_eq!(focal, ["gateway"], "the front door, not the config server everyone depends on");
    assert!(ir.nodes.iter().all(|n| n.id != "config"), "cross-cutting config server is left out of the figure");
    assert!(ir.edges.iter().filter(|e| e.primary()).all(|e| e.target != "config"));
    // The library's Kafka consumption is attributed to the deployable that links it.
    let lifted = ir
        .edges
        .iter()
        .find(|e| e.source == "kafka" && e.target == "stats")
        .expect("kafka delivers to stats via common");
    assert!(
        lifted.id.ends_with("--via-common")
            && lifted.label.as_deref().unwrap_or("").starts_with("delivers audit.events"),
        "{lifted:?}"
    );
}

#[test]
fn spring_modulith_packages_are_modules_and_events_flow_between_them() {
    let root = fixture("spring-modulith");
    let r = scan(&root, &ScanOptions { depth: Depth::Component, ..Default::default() }).unwrap();
    let app = unit(&r, "spring-modulith");
    assert_eq!(app.kind, UnitKind::HttpService, "`main(String... args)` is an entry point");
    let view = r.components.as_ref().unwrap();
    let modules: Vec<(&str, Option<&str>)> = view.modules.iter().map(|m| (m.id.as_str(), m.doc.as_deref())).collect();
    assert_eq!(
        modules,
        [
            ("spring-modulith.application", modules[0].1),
            ("spring-modulith.inventory", Some("Inventory: stock levels, updated when orders complete.")),
            ("spring-modulith.order", Some("Orders: placing and completing orders.")),
            ("spring-modulith.shipping", Some("Shipping: dispatches completed orders.")),
        ],
        "`com.example` is a package, not an examples directory"
    );
    let deps: Vec<(&str, &str, Vec<String>)> =
        view.dependencies.iter().map(|d| (d.source.as_str(), d.target.as_str(), d.events.clone())).collect();
    assert_eq!(
        deps,
        [
            ("spring-modulith.order", "spring-modulith.inventory", vec!["OrderCompleted".to_string()]),
            ("spring-modulith.shipping", "spring-modulith.inventory", vec![]),
            ("spring-modulith.shipping", "spring-modulith.order", vec![]),
        ],
        "the event, not the listener's import of `OrderCompleted`, links order and inventory"
    );
    let ev = &view.dependencies[0].evidence;
    assert_eq!((ev.file_path.as_str(), ev.start_line), ("src/main/java/com/example/order/OrderManagement.java", 16));
    assert_all_evidence_real(&root, &r);
    let ir = assert_draft_valid(&root, &r);
    let edge = ir.edges.iter().find(|e| e.edge_type == EdgeType::Event).expect("event edge");
    assert_eq!(edge.label.as_deref(), Some("OrderCompleted"));
}

#[test]
fn spring_request_flows_follow_feign_calls_to_the_line_that_makes_them() {
    let root = fixture("spring-cloud");
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    let flow =
        r.flows.iter().find(|f| f.entry == "accounts:GET /accounts/{name}").expect("flow for the accounts endpoint");
    let call = flow
        .steps
        .iter()
        .find(|s| s.from == "accounts" && s.to == "stats" && s.kind == autodoc_analyzer::trace::StepKind::Call)
        .unwrap_or_else(|| panic!("accounts → stats call in {:#?}", flow.steps));
    assert_eq!(call.label, "PUT /statistics/{accountName}");
    assert_eq!(call.evidence.file_path, "accounts/src/main/java/com/acme/accounts/web/AccountController.java");
    let text = std::fs::read_to_string(root.join(&call.evidence.file_path)).unwrap();
    let line = text.lines().nth(call.evidence.start_line as usize - 1).unwrap();
    assert!(line.contains("stats.update("), "cites the Feign call site, not the interface: {line}");
    assert!(flow.steps.iter().any(|s| s.from == "accounts" && s.to == "rabbitmq"), "{:#?}", flow.steps);
}

#[test]
fn runtime_topology_from_compose_overlays_and_kubernetes_manifests() {
    let root = fixture("k8s-deploy");
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    let topo = r.topology.as_ref().expect("topology");
    // The conditional template can't be read; the values-only one is rendered.
    assert!(topo.notes.iter().any(|n| n.contains("template logic") && n.contains("charts/orders")), "{:?}", topo.notes);
    let ids: Vec<&str> = topo.environments.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["compose:.", "k8s:charts/orders", "k8s:k8s"]);
    let chart = &topo.environments[1];
    assert_eq!(chart.name, "Helm chart (charts/orders/)");
    let rendered = chart.workloads.iter().find(|w| w.name == "orders-orders").expect("release name substituted");
    assert_eq!(rendered.image.as_deref(), Some("registry.example.com/acme/orders:1.4.2"));
    assert_eq!(rendered.replicas.as_deref(), Some("2"));
    assert_eq!(rendered.ports[0].target, "8080", "`default 8080` used for the unset value");
    assert_eq!(rendered.unit.as_deref(), Some("orders"));

    let compose = &topo.environments[0];
    let orders = compose.workloads.iter().find(|w| w.name == "orders").unwrap();
    assert_eq!(orders.unit.as_deref(), Some("orders"), "build ./orders runs the orders unit");
    assert_eq!(
        (orders.ports[0].published.as_deref(), orders.ports[0].target.as_str(), orders.ports[0].external),
        (Some("8080"), "8080", true)
    );
    assert!(orders.health_check);
    let db = compose.workloads.iter().find(|w| w.name == "db").unwrap();
    assert_eq!(
        (db.infra.as_deref(), db.volumes.as_slice()),
        (Some("postgres"), &["pgdata:/var/lib/postgresql/data".to_string()][..])
    );
    let dep = compose.links.iter().find(|l| l.kind == "depends-on").unwrap();
    assert_eq!((dep.from.as_str(), dep.to.as_str(), dep.evidence.start_line), ("orders", "db", 9));
    let port = compose.links.iter().find(|l| l.kind == "published-port").unwrap();
    assert_eq!((port.to.as_str(), port.label.as_str(), port.evidence.start_line), ("orders", "8080 → 8080", 5));
    assert_eq!(compose.overlays.len(), 1);
    assert_eq!(
        (compose.overlays[0].adds.as_slice(), compose.overlays[0].changes.as_slice()),
        (&["prometheus".to_string()][..], &["orders".to_string()][..])
    );

    let k8s = &topo.environments[2];
    let dep = k8s.workloads.iter().find(|w| w.name == "orders").unwrap();
    assert_eq!(dep.kind, "deployment");
    assert_eq!(dep.unit.as_deref(), Some("orders"), "image acme/orders deploys the orders unit");
    assert_eq!((dep.replicas.as_deref(), dep.namespace.as_deref(), dep.health_check), (Some("3"), Some("shop"), true));
    assert_eq!(dep.resources.as_deref(), Some("cpu 500m, memory 256Mi"));
    assert!(dep.ports.iter().any(|p| p.published.as_deref() == Some("80") && p.target == "8080"), "{:?}", dep.ports);
    let ingress = k8s.links.iter().find(|l| l.kind == "ingress").unwrap();
    assert_eq!((ingress.to.as_str(), ingress.label.as_str()), ("orders", "shop.example.com/api"));
    let db = k8s.workloads.iter().find(|w| w.name == "orders-db").unwrap();
    assert_eq!((db.kind.as_str(), db.infra.as_deref()), ("statefulset", Some("postgres")));
    let db_link = k8s.links.iter().find(|l| l.kind == "connects").expect("DATABASE_URL names orders-db");
    assert_eq!(
        (db_link.from.as_str(), db_link.to.as_str(), db_link.label.as_str()),
        ("orders", "orders-db", "via DATABASE_URL")
    );
    assert!(compose.links.iter().any(|l| l.kind == "connects" && l.from == "orders" && l.to == "db"));
    let cron = k8s.workloads.iter().find(|w| w.name == "nightly-report").unwrap();
    assert_eq!(
        (cron.kind.as_str(), cron.schedule.as_deref(), cron.unit.as_deref()),
        ("cronjob", Some("0 2 * * *"), Some("orders"))
    );
    for e in topo
        .environments
        .iter()
        .flat_map(|e| e.workloads.iter().map(|w| &w.evidence).chain(e.links.iter().map(|l| &l.evidence)))
    {
        assert_evidence_real(&root, e);
    }
    let text = std::fs::read_to_string(root.join("k8s/db.yaml")).unwrap();
    assert!(text.lines().nth(cron.evidence.start_line as usize - 1).unwrap().starts_with("kind: CronJob"));

    for env in &topo.environments {
        let d = autodoc_analyzer::topology::draft_topology_ir(
            &r,
            env,
            &DraftOptions { generated_at: Some("2026-01-01T00:00:00Z".into()), ..Default::default() },
        );
        let v = autodoc_validator::validate(
            &d.ir,
            &autodoc_validator::ValidateOptions { repo_root: Some(root.clone()), ..Default::default() },
        );
        assert!(v.valid, "{}: {:#?}", env.id, v.diagnostics);
        let drawn = d.ir.edges.iter().any(|e| e.source == "external");
        assert_eq!(drawn, env.links.iter().any(|l| l.from == "external"), "{}: entry traffic drawn", env.id);
    }
}

/// A unit at the repository root has an empty configuration prefix, and every
/// path starts with the empty string — so it collected every service's
/// `spring.application.name` as its own alias, and a `@FeignClient` naming any
/// of them resolved to the root instead of the service that answers to it.
/// The result was a dependency edge to the wrong unit, citing a real line.
#[test]
fn a_root_unit_does_not_answer_to_every_service_name() {
    let r = scan_container(&fixture("spring-monorepo"));

    let root_unit = r.containers.iter().find(|u| u.id == "spring-monorepo").unwrap_or_else(|| {
        panic!("root unit missing; have {:?}", r.containers.iter().map(|u| &u.id).collect::<Vec<_>>())
    });
    assert!(
        root_unit.aliases.is_empty(),
        "the root unit claimed names configured by the services below it: {:?}",
        root_unit.aliases
    );

    // The Feign call resolves to the service that configures that name.
    let edge =
        r.relationships.iter().find(|rel| rel.source == "gateway" && rel.target != "gateway").unwrap_or_else(|| {
            panic!(
                "no edge from the gateway; have {:?}",
                r.relationships.iter().map(|e| (&e.source, &e.target)).collect::<Vec<_>>()
            )
        });
    assert_eq!(edge.target, "accounts", "@FeignClient(\"account-service\") should reach the accounts service");
}
