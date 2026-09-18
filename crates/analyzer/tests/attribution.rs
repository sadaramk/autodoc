//! Integration: facts a literal reading of a large JVM system misses — access a
//! shared base class performs on behalf of its subclasses, a table name two
//! units each declare for themselves, route variants told apart by request
//! parameters, and response drift for callers that are not TypeScript.
//!
//! Every citation is read back from the file it points at.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use autodoc_analyzer::{scan, EvidenceRef, ScanOptions, ScanReport};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn report(name: &str) -> (PathBuf, ScanReport) {
    let root = fixture(name);
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    (root, r)
}

/// The cited lines exist and contain `needle`.
fn cites(root: &Path, e: &EvidenceRef, needle: &str) {
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        e.start_line >= 1 && e.start_line <= e.end_line && e.end_line as usize <= lines.len(),
        "{e:?} out of range"
    );
    let window = lines[e.start_line as usize - 1..e.end_line as usize].join("\n").to_lowercase();
    assert!(
        window.contains(&needle.to_lowercase()),
        "{}:{} is {window:?}, expected {needle:?}",
        e.file_path,
        e.start_line
    );
}

#[test]
fn a_generic_dao_base_writes_for_each_concrete_dao() {
    let (root, r) = report("attribution");
    let m = r.data.as_ref().expect("data model");
    // The base names no entity; the DAOs that extend it name two.
    for (table, sub) in [("device", "JpaDeviceDao"), ("asset", "JpaAssetDao")] {
        let e = m.entities.iter().find(|e| e.table == table).expect("entity");
        let note = format!("through `{sub}`");
        let mut cited: Vec<(u32, &str)> = Vec::new();
        for a in &e.writes {
            assert_eq!(a.evidence.note.as_deref(), Some(note.as_str()), "{table} write {:?}", a.evidence);
            cited.push((a.evidence.start_line, a.evidence.file_path.as_str()));
        }
        cited.sort();
        let lines: Vec<u32> = cited.iter().map(|(l, _)| *l).collect();
        assert!(cited.iter().all(|(_, p)| p.ends_with("JpaAbstractDao.java")), "{table}: {cited:?}");
        assert_eq!(lines.len(), 3, "{table}: save, persist and delete are three writes, got {cited:?}");

        // Each write is a real persistence call on the cited line.
        for w in &e.writes {
            let text = std::fs::read_to_string(root.join(&w.evidence.file_path)).unwrap();
            let line = text.lines().nth(w.evidence.start_line as usize - 1).unwrap_or_default().to_string();
            assert!(
                ["getRepository().save(entity)", "entityManager.persist(entity)", "repository.deleteById(id)"]
                    .iter()
                    .any(|n| line.contains(n)),
                "{table} write at {}:{} is {line:?}",
                w.evidence.file_path,
                w.evidence.start_line
            );
        }
        let read = e.reads.first().expect("findById is a read");
        assert_eq!(read.evidence.note.as_deref(), Some(note.as_str()));
        cites(&root, &read.evidence, "getRepository().findById(id)");
    }
}

#[test]
fn a_base_class_nobody_extends_attributes_nothing() {
    let (_, r) = report("attribution");
    let m = r.data.as_ref().expect("data model");
    // `JpaAbstractAuditDao.purge` deletes through the same accessor shape, but no
    // subclass binds its type parameters, so no entity may claim the call.
    for e in &m.entities {
        for a in e.reads.iter().chain(&e.writes) {
            assert!(
                !a.evidence.file_path.contains("JpaAbstractAuditDao"),
                "{} claims {}:{}, which names no entity",
                e.table,
                a.evidence.file_path,
                a.evidence.start_line
            );
        }
    }
}

#[test]
fn two_units_declaring_one_table_name_are_two_tables() {
    let (root, r) = report("attribution");
    let m = r.data.as_ref().expect("data model");
    let mut found: Vec<(&str, Vec<&str>, Vec<&str>)> = m
        .entities
        .iter()
        .filter(|e| e.table == "customer")
        .map(|e| {
            (
                e.id.as_str(),
                e.units.iter().map(String::as_str).collect(),
                e.columns.iter().map(|c| c.name.as_str()).collect(),
            )
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        vec![
            ("entity:orders.customer", vec!["orders"], vec!["id", "email", "billing_address"]),
            ("entity:reporting.customer", vec!["reporting"], vec!["id", "cohort", "first_seen_at"]),
        ]
    );
    // Both declarations are real, in the unit that owns them.
    for e in m.entities.iter().filter(|e| e.table == "customer") {
        cites(&root, &e.evidence, "class Customer");
        assert!(e.evidence.file_path.starts_with(e.units[0].as_str()), "{:?}", e.evidence);
    }
}

#[test]
fn route_variants_selected_by_request_parameters_are_separate_operations() {
    let (root, r) = report("attribution");
    let api = r.api.as_ref().expect("api");
    let ids: BTreeSet<&str> = api.operations.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids.len(), api.operations.len(), "operation ids are unique");
    let mut variants: Vec<(&str, Option<&str>)> = api
        .operations
        .iter()
        .filter(|o| o.path == "/customers")
        .map(|o| (o.id.as_str(), o.selector.as_deref()))
        .collect();
    variants.sort();
    assert_eq!(
        variants,
        vec![
            ("orders:GET /customers", None),
            ("orders:GET /customers?accountNumber", Some("accountNumber")),
            ("orders:GET /customers?email", Some("email")),
        ]
    );
    for o in api.operations.iter().filter(|o| o.selector.is_some()) {
        cites(&root, &o.handler.evidence, "params");
    }
}

#[test]
fn response_drift_is_reported_for_java_python_and_go_callers() {
    let (root, r) = report("client-drift");
    let api = r.api.as_ref().expect("api");
    let mut drift: Vec<(&str, &str, Vec<&str>)> = api
        .client_calls
        .iter()
        .filter(|c| !c.drift.is_empty())
        .map(|c| (c.unit.as_str(), c.evidence.file_path.as_str(), c.drift.iter().map(String::as_str).collect()))
        .collect();
    drift.sort();
    assert_eq!(
        drift,
        vec![
            // Feign interface return type.
            ("billing", "billing/src/main/java/com/acme/billing/AccountClient.java", vec!["currency"]),
            // `RestTemplate.exchange(…, new ParameterizedTypeReference<List<AccountSummary>>() {})`.
            ("billing", "billing/src/main/java/com/acme/billing/AccountDirectory.java", vec!["tier"]),
            // A pydantic model built from the body, and a key read straight off it.
            ("reporting", "reporting/reporting/accounts.py", vec!["openedAt"]),
            ("reporting", "reporting/reporting/accounts.py", vec!["overdraft"]),
            // `json.Unmarshal(body, &account)`.
            ("sync", "sync/main.go", vec!["lastLogin"]),
        ]
    );

    // Both halves of the claim are citable: where the caller reads, and where the
    // operation declares its response.
    for c in api.client_calls.iter().filter(|c| !c.drift.is_empty()) {
        cites(&root, &c.evidence, "account");
        let op = api
            .operations
            .iter()
            .find(|o| Some(o.id.as_str()) == c.operation.as_deref())
            .expect("drift is only reported against a matched operation");
        let model_id = op.response.as_ref().and_then(|t| t.model.as_deref()).expect("declared response model");
        let model = api.models.iter().find(|m| m.id == model_id).expect("declared model is published");
        cites(&root, &model.evidence, "record AccountView");
        for field in &c.drift {
            assert!(!model.fields.iter().any(|f| &f.name == field), "{field} is declared after all");
        }
    }
}
