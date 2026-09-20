//! Integration: capability groups (controller / router / tag / path) and the
//! capability map drafted from them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nunki_analyzer::capability::groups_of;
use nunki_analyzer::{draft_capability_ir, scan, DraftOptions, ScanOptions, ScanReport};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn report(name: &str) -> (PathBuf, ScanReport) {
    let root = fixture(name);
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    (root, r)
}

/// `"METHOD path" → group` for one unit.
fn groups(r: &ScanReport, unit: &str) -> BTreeMap<String, String> {
    let api = r.api.as_ref().unwrap();
    groups_of(api, unit)
        .into_iter()
        .flat_map(|(g, ops)| ops.into_iter().map(move |o| (format!("{} {}", o.method, o.path), g.clone())))
        .collect()
}

/// (fixture, unit, [("METHOD path", expected group)])
type Case = (&'static str, &'static str, &'static [(&'static str, &'static str)]);

#[test]
fn groups_follow_controllers_routers_tags_and_paths() {
    let cases: &[Case] = &[
        ("api-frameworks/spring-mvc", "orders-service", &[("GET /api/orders/{id}", "Order")]),
        ("api-frameworks/spring-mvc", "inventory-service", &[("POST /inventory/{sku}/reservations", "Inventory")]),
        ("api-frameworks/quarkus-jaxrs", "quarkus-jaxrs", &[("GET /api/fruits", "Fruit")]),
        ("api-frameworks/micronaut", "micronaut", &[("DELETE /v1/pets/{id}", "Pet")]),
        ("api-frameworks/spring-webflux-functional", "spring-webflux-functional", &[("GET /api/books", "Book")]),
        ("api-frameworks/nestjs", "nestjs", &[("POST /api/orders", "Orders")]),
        ("api-frameworks/express", "express", &[("GET /api/users/{id}", "Users")]),
        // `router.go` names a role, not an area: the path decides, admin routes apart.
        (
            "api-frameworks/go-chi",
            "go-chi",
            &[("GET /api/tasks", "Tasks"), ("DELETE /api/admin/tasks/{taskID}", "Admin")],
        ),
        ("api-frameworks/gin", "gin", &[("GET /api/v1/books", "Books")]),
        ("api-frameworks/actix", "actix", &[("POST /api/v1/items/{sku}/restock", "Items")]),
        ("api-frameworks/axum", "axum", &[("GET /api/accounts", "Accounts")]),
        ("api-frameworks/fastapi", "fastapi", &[("GET /api/v2/products", "Products")]),
        ("api-frameworks/flask", "flask", &[("GET /api/posts", "Posts")]),
        ("polyglot-shop", "api-gateway", &[("POST /checkout", "Checkout"), ("GET /catalog", "Catalog")]),
        ("polyglot-shop", "payments", &[("POST /charges", "Charges")]),
    ];
    for (fx, unit, expected) in cases {
        let (_, r) = report(fx);
        let g = groups(&r, unit);
        for (op, group) in *expected {
            assert_eq!(g.get(*op).map(String::as_str), Some(*group), "{fx} {unit}: {op} in {g:#?}");
        }
    }
    // Every operation has a group.
    let (_, r) = report("api-frameworks/fastapi");
    assert!(r.api.as_ref().unwrap().operations.iter().all(|o| o.group.as_deref().is_some_and(|g| !g.is_empty())));
}

#[test]
fn capability_map_shows_callers_groups_and_what_they_touch() {
    let (root, r) = report("polyglot-shop");
    let d = draft_capability_ir(
        &r,
        "api-gateway",
        &DraftOptions { generated_at: Some("2026-01-01T00:00:00Z".into()), ..Default::default() },
    )
    .expect("api-gateway has operations");
    let ir = &d.ir;
    let ids: Vec<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains(&"cap-checkout") && ids.contains(&"cap-catalog"), "{ids:?}");
    // The web client calls checkout from the repository.
    let call = ir.edges.iter().find(|e| e.source == "web" && e.target == "cap-checkout").expect("web → Checkout");
    assert!(call.evidence.is_some());
    // Checkout writes orders and publishes; the checkout group is the focal area.
    let touched: Vec<(&str, Option<&str>)> = ir
        .edges
        .iter()
        .filter(|e| e.source == "cap-checkout")
        .map(|e| (e.target.as_str(), e.label.as_deref()))
        .collect();
    assert!(touched.iter().any(|(t, l)| *t == "postgres" && l.is_some_and(|l| l.contains("writes"))), "{touched:?}");
    assert!(touched.iter().any(|(t, l)| *t == "kafka" && l.is_some_and(|l| l.contains("publishes"))), "{touched:?}");
    assert_eq!(
        ir.nodes.iter().filter(|n| n.is_key_focal_point).map(|n| n.id.as_str()).collect::<Vec<_>>(),
        ["cap-checkout"]
    );
    let n = ir.nodes.iter().find(|n| n.id == "cap-checkout").unwrap();
    assert_eq!(n.subtitle.as_deref(), Some("1 operation"));
    assert_eq!(n.tech_stack.as_deref(), Some("POST 1"));

    let v = nunki_validator::validate(
        ir,
        &nunki_validator::ValidateOptions { repo_root: Some(root.clone()), ..Default::default() },
    );
    assert!(v.valid, "{:#?}", v.diagnostics);

    // Services without in-repo callers are reached by outside clients, not left unconnected.
    let (root, r) = report("api-frameworks/spring-mvc");
    let d = draft_capability_ir(&r, "orders-service", &DraftOptions::default()).unwrap();
    assert!(
        d.ir.nodes.iter().any(|n| n.id == "external-clients"),
        "{:?}",
        d.ir.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
    let v = nunki_validator::validate(
        &d.ir,
        &nunki_validator::ValidateOptions { repo_root: Some(root), ..Default::default() },
    );
    assert!(v.valid, "{:#?}", v.diagnostics);
}

#[test]
fn many_groups_fold_into_other_within_the_density_budget() {
    let (root, mut r) = report("api-frameworks/spring-mvc");
    // Spread the orders operations over many synthetic groups.
    let api = r.api.as_mut().unwrap();
    let template = api.operations.iter().find(|o| o.unit == "orders-service").unwrap().clone();
    for i in 0..40 {
        let mut op = template.clone();
        op.id = format!("orders-service:GET /api/area{i}");
        op.path = format!("/api/area{i}");
        op.group = Some(format!("Area {i:02}"));
        api.operations.push(op);
    }
    let d = draft_capability_ir(&r, "orders-service", &DraftOptions::default()).unwrap();
    assert!(
        d.ir.nodes.len() + d.ir.edges.len() <= nunki_ir::element_budget(),
        "{} + {}",
        d.ir.nodes.len(),
        d.ir.edges.len()
    );
    assert!(
        d.ir.nodes.iter().any(|n| n.label.starts_with("Other (")),
        "{:?}",
        d.ir.nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
    );
    assert!(d.notes.iter().any(|n| n.contains("drawn as one `Other` node")), "{:?}", d.notes);
    let focal: Vec<&str> = d.ir.nodes.iter().filter(|n| n.is_key_focal_point).map(|n| n.label.as_str()).collect();
    assert!(focal.len() == 1 && !focal[0].starts_with("Other ("), "{focal:?}");
    let v = nunki_validator::validate(
        &d.ir,
        &nunki_validator::ValidateOptions { repo_root: Some(root), ..Default::default() },
    );
    assert!(v.valid, "{:#?}", v.diagnostics);
}
