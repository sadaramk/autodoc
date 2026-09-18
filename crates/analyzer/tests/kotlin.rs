//! Integration: Kotlin support end to end — the facts the extractor reads from
//! `.kt` sources, unit classification, HTTP contracts (Spring MVC and Ktor),
//! and the data model (JPA, Spring Data MongoDB, Exposed) with its lifecycles.
//!
//! Every citation is read back off disk and must name what it is cited for: the
//! point of the whole engine is that a reader can follow the line.

use std::path::{Path, PathBuf};

use autodoc_analyzer::api::ApiModel;
use autodoc_analyzer::data::DataModel;
use autodoc_analyzer::extract::{self, FileFacts};
use autodoc_analyzer::lang::{Grammar, Language};
use autodoc_analyzer::scan::EvidenceRef;
use autodoc_analyzer::{scan, ScanOptions, ScanReport, UnitKind};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn report(name: &str) -> (PathBuf, ScanReport) {
    let root = fixture(name);
    let r = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).expect("scan");
    (root, r)
}

fn facts(root: &Path, rel: &str) -> FileFacts {
    let text = std::fs::read_to_string(root.join(rel)).unwrap_or_else(|_| panic!("{rel} missing"));
    let f = extract::extract(Grammar::Kotlin, Language::Kotlin, rel, &text);
    assert!(!f.has_syntax_errors, "{rel} did not parse cleanly");
    f
}

fn norm(s: &str) -> String {
    s.to_lowercase().replace(['_', '-'], "")
}

/// The cited lines exist and mention one of `needles`.
fn cites(root: &Path, e: &EvidenceRef, needles: &[&str]) {
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        e.start_line >= 1 && e.start_line <= e.end_line && e.end_line as usize <= lines.len(),
        "{e:?} out of range in {}",
        e.file_path
    );
    let window = norm(&lines[e.start_line as usize - 1..e.end_line as usize].join("\n"));
    assert!(
        needles.iter().any(|n| window.contains(&norm(n))),
        "{}:{} mentions none of {needles:?}",
        e.file_path,
        e.start_line
    );
}

/// Every citation in the API model points at a real line, naming its symbol.
fn assert_api_evidence(root: &Path, api: &ApiModel) {
    let check = |e: &EvidenceRef| {
        let text =
            std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            e.start_line >= 1 && e.start_line <= e.end_line && e.end_line as usize <= lines.len(),
            "{e:?} out of range"
        );
        if let Some(sym) = &e.symbol_name {
            let window = lines[e.start_line as usize - 1..e.end_line as usize].join("\n");
            assert!(window.contains(sym.as_str()), "{sym} not within {e:?}");
        }
    };
    for op in &api.operations {
        check(&op.evidence);
        check(&op.handler.evidence);
        op.params.iter().for_each(|p| check(&p.evidence));
        op.errors.iter().for_each(|e| check(&e.evidence));
        op.auth.iter().for_each(|a| check(&a.evidence));
        for t in [&op.request_body, &op.response].into_iter().flatten() {
            if let Some(m) = &t.model {
                assert!(api.models.iter().any(|x| x.id == *m), "{} references missing model {m}", op.id);
            }
        }
    }
    for m in &api.models {
        check(&m.evidence);
        for f in &m.fields {
            check(&f.evidence);
            f.rules.iter().for_each(|r| check(&r.evidence));
        }
    }
    api.excluded.iter().for_each(|x| check(&x.evidence));
}

/// Every citation in the data model names the entity, column or relation.
fn assert_data_evidence(root: &Path, m: &DataModel) {
    for e in &m.entities {
        cites(root, &e.evidence, &[&e.name, &e.table]);
        for c in &e.columns {
            cites(root, &c.evidence, &[&c.name, c.name.trim_end_matches("_id"), &e.name]);
        }
        for r in &e.relations {
            let target = m.entities.iter().find(|t| t.id == r.target).expect("relation targets a known entity");
            cites(root, &r.evidence, &[&r.via, &target.table, &target.name]);
        }
        let mut names: Vec<String> = vec![e.name.clone(), e.table.clone()];
        names.extend(e.columns.iter().map(|c| c.name.clone()));
        // Access through a DAO or a repository names that instead.
        for a in e.reads.iter().chain(&e.writes) {
            let mut needles: Vec<&str> = names.iter().map(String::as_str).collect();
            let note = a.evidence.note.clone().unwrap_or_default();
            let through = note.trim_start_matches("through `").trim_end_matches('`').to_string();
            needles.push("repository");
            needles.push("audit");
            if !through.is_empty() {
                needles.push(&through);
            }
            cites(root, &a.evidence, &needles);
        }
    }
    for s in &m.state_machines {
        for st in &s.states {
            cites(root, &st.evidence, &[&st.name]);
        }
        for t in &s.transitions {
            cites(root, &t.evidence, &[&t.to]);
        }
    }
}

// ---------------------------------------------------------------------------
// Facts
// ---------------------------------------------------------------------------

#[test]
fn kotlin_facts_carry_package_imports_symbols_and_annotations() {
    let root = fixture("kotlin/spring-boot-kotlin");
    let f = facts(&root, "src/main/kotlin/com/acme/shop/web/OrderController.kt");
    assert_eq!(f.package.as_deref(), Some("com.acme.shop.web"));
    assert!(
        f.imports.iter().any(|i| i.specifier == "org.springframework.web.bind.annotation.RestController"),
        "imports: {:?}",
        f.imports.iter().map(|i| &i.specifier).collect::<Vec<_>>()
    );
    let names: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"OrderController") && names.contains(&"PlaceOrder") && names.contains(&"list"),
        "{names:?}"
    );
    let controller = f.symbols.iter().find(|s| s.name == "OrderController").expect("class symbol");
    assert_eq!(controller.doc.as_deref(), Some("Orders the shop serves over HTTP."));

    // Class-level annotations survive the grammar's habit of parsing leading
    // annotations as an expression of their own.
    let ann = |name: &str| f.annotations.iter().find(|a| a.name == name).unwrap_or_else(|| panic!("no @{name}"));
    assert_eq!(
        (ann("RestController").target_kind.as_str(), ann("RestController").target.as_str()),
        ("class", "OrderController")
    );
    assert_eq!(ann("RequestMapping").arguments, "\"/orders\"");
    assert_eq!(ann("GetMapping").target_kind, "method");
    // `@field:` and `@Valid` use-site targets land on the declaration they annotate.
    let size = f.annotations.iter().find(|a| a.name == "Size").expect("@field:Size");
    assert_eq!((size.target_kind.as_str(), size.target.as_str()), ("field", "customerName"));
    assert_eq!(
        f.annotations.iter().find(|a| a.name == "Valid").map(|a| (a.target_kind.as_str(), a.target.as_str())),
        Some(("parameter", "body"))
    );
}

#[test]
fn kotlin_facts_carry_entry_points_and_application_events() {
    let root = fixture("kotlin/spring-boot-kotlin");
    let app = facts(&root, "src/main/kotlin/com/acme/shop/ShopApplication.kt");
    let entries: Vec<(&str, &str)> = app.entry_points.iter().map(|e| (e.symbol.as_str(), e.reason.as_str())).collect();
    assert!(
        entries.iter().any(|(s, r)| *s == "ShopApplication" && r.contains("SpringBootApplication")),
        "@SpringBootApplication is the entry point, got {entries:?}"
    );

    let service = facts(&root, "src/main/kotlin/com/acme/shop/domain/OrderService.kt");
    let published: Vec<(&str, &str)> = service
        .events
        .iter()
        .filter(|e| e.kind == "publish")
        .map(|e| (e.event_type.as_str(), e.method.as_str()))
        .collect();
    assert_eq!(published, vec![("OrderPlaced", "place")]);

    let listener = facts(&root, "src/main/kotlin/com/acme/shop/audit/AuditListener.kt");
    let heard: Vec<(&str, &str)> = listener
        .events
        .iter()
        .filter(|e| e.kind == "listen")
        .map(|e| (e.event_type.as_str(), e.method.as_str()))
        .collect();
    assert_eq!(heard, vec![("OrderPlaced", "onOrderPlaced")]);

    let ktor = facts(&fixture("api-frameworks/ktor"), "src/main/kotlin/com/acme/catalog/Application.kt");
    assert!(
        ktor.entry_points.iter().any(|e| e.reason.contains("embeddedServer")),
        "Ktor's `embeddedServer` is the entry point: {:?}",
        ktor.entry_points
    );
}

#[test]
fn kotlin_files_never_panic_on_odd_input() {
    for src in [
        "",
        "@",
        "class",
        "package",
        "class A(val b:",
        "fun main() { }",
        "@Entity @Table( class Broken(@Id val",
        "object X : Table(\"y\") { val z = varchar( }",
        "// 🎯 unicode and a raw string\nval s = \"\"\"x\"\"\"\n",
    ] {
        let f = extract::extract(Grammar::Kotlin, Language::Kotlin, "x.kt", src);
        assert!(f.line_count as usize <= src.lines().count().max(1));
    }
}

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

#[test]
fn kotlin_spring_boot_is_a_service_and_a_kotlin_library_stays_a_library() {
    let (_, r) = report("kotlin/kotlin-modules");
    let unit = |id: &str| r.containers.iter().find(|u| u.id == id).unwrap_or_else(|| panic!("no unit {id}"));
    assert_eq!(unit("app").kind, UnitKind::HttpService);
    assert_eq!(unit("app").language, Language::Kotlin);
    assert!(unit("app").frameworks.iter().any(|f| f == "Spring MVC"), "{:?}", unit("app").frameworks);
    // Framework code in a shared module does not make it deployable.
    assert_eq!(unit("pricing").kind, UnitKind::Library);

    // The Gradle `project(":pricing")` edge is cited at the import that uses it.
    let edge =
        r.relationships.iter().find(|e| e.source == "app" && e.target == "pricing").expect("app depends on pricing");
    let ev = edge.evidence.first().expect("edge evidence");
    assert_eq!(ev.file_path, "app/src/main/kotlin/com/acme/quotes/web/QuoteController.kt");

    let (root, r) = report("api-frameworks/ktor");
    let ktor = r.containers.iter().find(|u| u.id == "ktor").expect("ktor unit");
    assert_eq!((ktor.kind, ktor.language), (UnitKind::HttpService, Language::Kotlin));
    assert!(ktor.frameworks.iter().any(|f| f == "Ktor"), "{:?}", ktor.frameworks);
    cites(&root, &ktor.entry_points[0], &["embeddedServer"]);
}

// ---------------------------------------------------------------------------
// API contracts
// ---------------------------------------------------------------------------

#[test]
fn kotlin_spring_controller_contracts() {
    let (root, r) = report("kotlin/spring-boot-kotlin");
    let api = r.api.expect("api model");
    assert_api_evidence(&root, &api);
    let ids: Vec<&str> = api.operations.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "spring-boot-kotlin:GET /api/orders",
            "spring-boot-kotlin:POST /api/orders",
            "spring-boot-kotlin:GET /api/orders/{id}",
        ],
        "`server.servlet.context-path: /api` prefixes every route"
    );
    let op = |id: &str| api.operations.iter().find(|o| o.id == id).unwrap_or_else(|| panic!("no {id}"));

    let list = op("spring-boot-kotlin:GET /api/orders");
    let params: Vec<(&str, &str, bool)> =
        list.params.iter().map(|p| (p.name.as_str(), p.location.as_str(), p.required)).collect();
    assert_eq!(params, vec![("status", "query", false), ("limit", "query", false)]);
    assert_eq!(
        list.params[0].rules.iter().map(|r| r.statement.as_str()).collect::<Vec<_>>(),
        vec!["one of: NEW, PAID, SHIPPED, CANCELLED"],
        "a Kotlin enum parameter states its values"
    );
    let resp = list.response.as_ref().expect("response");
    assert!(resp.collection && resp.model.as_deref() == Some("spring-boot-kotlin:OrderView"));
    assert_eq!(list.auth.iter().map(|a| a.detail.as_str()).collect::<Vec<_>>(), vec!["hasRole('ADMIN')"]);

    let place = op("spring-boot-kotlin:POST /api/orders");
    assert_eq!(place.success_status, Some(201), "@ResponseStatus(HttpStatus.CREATED)");
    assert_eq!(place.request_body.as_ref().and_then(|b| b.model.as_deref()), Some("spring-boot-kotlin:PlaceOrder"));

    let by_id = op("spring-boot-kotlin:GET /api/orders/{id}");
    assert_eq!(by_id.params.iter().map(|p| (p.name.as_str(), p.required)).collect::<Vec<_>>(), vec![("id", true)]);
    let err = by_id.errors.first().expect("ResponseStatusException");
    assert_eq!((err.status, err.message.as_deref()), (Some(404), Some("order not found")));

    // `@field:` Bean Validation and Kotlin nullability describe the body model.
    let body = api.models.iter().find(|m| m.id == "spring-boot-kotlin:PlaceOrder").expect("PlaceOrder");
    let fields: Vec<(&str, &str, bool)> =
        body.fields.iter().map(|f| (f.name.as_str(), f.type_name.as_str(), f.required)).collect();
    assert_eq!(fields, vec![("customerName", "String", true), ("email", "String?", false), ("note", "String?", false)]);
    assert_eq!(
        body.fields[0].rules.iter().map(|r| r.statement.as_str()).collect::<Vec<_>>(),
        vec!["must not be blank", "at most 80 characters"]
    );
    assert_eq!(
        body.fields[1].rules.iter().map(|r| r.statement.as_str()).collect::<Vec<_>>(),
        vec!["must be a valid email"]
    );
}

#[test]
fn ktor_routing_dsl_contracts() {
    let (root, r) = report("api-frameworks/ktor");
    let api = r.api.expect("api model");
    assert_api_evidence(&root, &api);
    let ids: Vec<&str> = api.operations.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["ktor:GET /books", "ktor:POST /books", "ktor:GET /books/{id}"],
        "`route(\"/books\")` prefixes"
    );
    let op = |id: &str| api.operations.iter().find(|o| o.id == id).unwrap_or_else(|| panic!("no {id}"));

    assert_eq!(
        op("ktor:GET /books").params.iter().map(|p| (p.name.as_str(), p.location.as_str())).collect::<Vec<_>>(),
        vec![("q", "query")],
        "`call.request.queryParameters[\"q\"]`"
    );
    assert_eq!(
        op("ktor:GET /books/{id}")
            .params
            .iter()
            .map(|p| (p.name.as_str(), p.location.as_str(), p.required))
            .collect::<Vec<_>>(),
        vec![("id", "path", true)]
    );
    let post = op("ktor:POST /books");
    assert_eq!(post.success_status, Some(201), "`call.respond(HttpStatusCode.Created, …)`");
    assert_eq!(
        post.request_body.as_ref().and_then(|b| b.model.as_deref()),
        Some("ktor:NewBook"),
        "`call.receive<NewBook>()`"
    );
    assert_eq!(
        post.auth.iter().map(|a| (a.kind.as_str(), a.detail.as_str())).collect::<Vec<_>>(),
        vec![("authenticated", "jwt")],
        "`authenticate(\"jwt\") {{ … }}`"
    );
    assert!(
        api.excluded.iter().any(|x| x.operation.contains("/healthz")),
        "probes are excluded: {:?}",
        api.excluded.iter().map(|x| &x.operation).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[test]
fn kotlin_jpa_and_mongo_entities_with_repository_access() {
    let (root, r) = report("kotlin/spring-boot-kotlin");
    let m = r.data.expect("data model");
    assert_data_evidence(&root, &m);
    let tables: Vec<&str> = m.entities.iter().map(|e| e.table.as_str()).collect();
    assert_eq!(tables, vec!["audit_log", "customer", "order_items", "orders"]);
    let entity = |t: &str| m.entities.iter().find(|e| e.table == t).unwrap_or_else(|| panic!("no {t}"));
    let col = |t: &str, n: &str| {
        entity(t).columns.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no {t}.{n}")).clone()
    };

    // Spring Boot's naming strategy snake-cases what `@Column` doesn't name.
    let orders = entity("orders");
    assert_eq!(orders.source, "jpa");
    assert_eq!(
        orders.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "customer_name", "total_amount", "status", "customer_id", "placed_at"]
    );
    assert!(col("orders", "id").primary_key && !col("orders", "id").nullable);
    assert_eq!(col("orders", "customer_name").type_name, "varchar(80)");
    // Nullability comes from the Kotlin type, not only from `@Column`.
    assert!(!col("orders", "customer_name").nullable && col("orders", "placed_at").nullable);
    assert_eq!(col("orders", "status").type_name, "varchar", "@Enumerated(EnumType.STRING)");
    assert_eq!(col("orders", "customer_id").references.as_deref(), Some("customer.id"));
    let rels: Vec<(&str, &str, &str)> =
        orders.relations.iter().map(|x| (x.kind.as_str(), x.target.as_str(), x.via.as_str())).collect();
    assert_eq!(
        rels,
        vec![("many-to-one", "entity:customer", "customer_id"), ("one-to-many", "entity:order_items", "order")],
        "`@OneToMany(mappedBy = \"order\")` is the inverse side"
    );

    // A `@MappedSuperclass` and an `@Embeddable` flatten into the owning table.
    let items = entity("order_items");
    assert_eq!(
        items.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "created_at", "order_id", "sku", "quantity", "amount", "currency"]
    );
    assert_eq!(
        col("order_items", "created_at").evidence.file_path,
        "src/main/kotlin/com/acme/shop/domain/Auditable.kt"
    );
    assert_eq!(col("order_items", "amount").type_name, "numeric(12,2)");
    assert_eq!(col("order_items", "amount").evidence.file_path, "src/main/kotlin/com/acme/shop/domain/Money.kt");
    assert_eq!(col("order_items", "quantity").constraints, vec!["> 0"], "@field:Positive");
    assert!(!col("order_items", "order_id").nullable, "@JoinColumn(nullable = false)");
    assert_eq!(col("order_items", "id").default.as_deref(), Some("generated"));

    // Spring Data MongoDB: the collection name and `@Field` wire names.
    let audit = entity("audit_log");
    assert_eq!(audit.source, "spring-data-mongodb");
    assert_eq!(
        audit.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "order_id", "action", "at"]
    );

    // Repository calls are the reads and writes, at the line that makes them.
    let sites = |a: &[autodoc_analyzer::data::Access]| -> Vec<(String, u32)> {
        let mut v: Vec<(String, u32)> =
            a.iter().map(|x| (x.evidence.file_path.clone(), x.evidence.start_line)).collect();
        v.sort();
        v
    };
    assert!(
        sites(&orders.writes).contains(&("src/main/kotlin/com/acme/shop/domain/OrderService.kt".into(), 21)),
        "`repository.save(order)`: {:?}",
        sites(&orders.writes)
    );
    assert!(
        sites(&orders.reads).contains(&("src/main/kotlin/com/acme/shop/domain/OrderService.kt".into(), 24)),
        "`repository.findById(id)`: {:?}",
        sites(&orders.reads)
    );
    assert!(
        sites(&orders.writes).contains(&("src/main/kotlin/com/acme/shop/jobs/StaleOrders.kt".into(), 15)),
        "`@Modifying @Query` repository method writes: {:?}",
        sites(&orders.writes)
    );
    assert!(
        sites(&orders.reads).contains(&("src/main/kotlin/com/acme/shop/jobs/StaleOrders.kt".into(), 20)),
        "a derived finder reads: {:?}",
        sites(&orders.reads)
    );
    assert!(!audit.writes.is_empty(), "`audit.save(AuditEvent(…))` writes the document");

    // The status field's lifecycle, from the enum and the assignments.
    let sm = m.state_machines.iter().find(|s| s.subject == "orders.status").expect("orders.status lifecycle");
    assert_eq!(
        sm.states.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["NEW", "PAID", "SHIPPED", "CANCELLED"]
    );
    assert!(sm.states[0].initial, "`= OrderStatus.NEW` is the initial state");
    let to: Vec<&str> = sm.transitions.iter().map(|t| t.to.as_str()).collect();
    assert!(to.contains(&"PAID") && to.contains(&"CANCELLED"), "{to:?}");
}

#[test]
fn exposed_table_objects_columns_and_access() {
    let (root, r) = report("kotlin/exposed-ledger");
    let m = r.data.expect("data model");
    assert_data_evidence(&root, &m);
    assert_eq!(
        m.entities.iter().map(|e| e.table.as_str()).collect::<Vec<_>>(),
        vec!["accounts", "entries", "entry_tags"]
    );
    let entity = |t: &str| m.entities.iter().find(|e| e.table == t).unwrap_or_else(|| panic!("no {t}"));
    let col = |t: &str, n: &str| {
        entity(t).columns.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no {t}.{n}")).clone()
    };

    let accounts = entity("accounts");
    assert_eq!((accounts.source.as_str(), accounts.name.as_str()), ("exposed", "Accounts"));
    // `UUIDTable` brings a generated key of its own.
    assert_eq!(
        (
            col("accounts", "id").type_name.as_str(),
            col("accounts", "id").primary_key,
            col("accounts", "id").default.as_deref()
        ),
        ("uuid", true, Some("generated"))
    );
    assert!(col("accounts", "name").unique, ".uniqueIndex()");
    assert_eq!(col("accounts", "balance").type_name, "numeric(14,2)");
    assert_eq!(col("accounts", "balance").default.as_deref(), Some("ZERO"));
    assert!(col("accounts", "closed_at").nullable, ".nullable()");

    let entries = entity("entries");
    assert_eq!(col("entries", "id").type_name, "integer", "IntIdTable");
    // A reference takes the type of the key it points at.
    assert_eq!(
        (col("entries", "account_id").type_name.as_str(), col("entries", "account_id").references.as_deref()),
        ("uuid", Some("accounts.id"))
    );
    assert_eq!(col("entries", "memo").type_name, "text");
    assert_eq!(col("entries", "booked_at").default.as_deref(), Some("set by the application"), ".clientDefault {{ }}");
    assert_eq!(
        entries.relations.iter().map(|x| (x.kind.as_str(), x.target.as_str())).collect::<Vec<_>>(),
        vec![("many-to-one", "entity:accounts")]
    );

    // `override val primaryKey = PrimaryKey(entry, tag)`: unique together only.
    let tags = entity("entry_tags");
    assert!(col("entry_tags", "entry_id").primary_key && col("entry_tags", "tag").primary_key);
    assert!(!col("entry_tags", "entry_id").unique, "one column of a composite key is not unique alone");
    assert_eq!(tags.relations.len(), 1);

    // DSL statements and the DAO that fronts them.
    let at = |a: &[autodoc_analyzer::data::Access], line: u32| a.iter().any(|x| x.evidence.start_line == line);
    assert!(
        at(&entries.writes, 14),
        "`Entries.insert {{ }}`: {:?}",
        entries.writes.iter().map(|a| a.evidence.start_line).collect::<Vec<_>>()
    );
    assert!(at(&entries.reads, 24), "`Entries.selectAll()`");
    assert!(at(&accounts.writes, 19), "`Accounts.update({{ }}) {{ }}`");
    let dao = accounts.reads.iter().find(|a| a.evidence.note.is_some()).expect("DAO access carries a note");
    assert_eq!(dao.evidence.note.as_deref(), Some("through `AccountEntity`"));

    // The enum column's lifecycle, initial state from the column default.
    let sm = m.state_machines.iter().find(|s| s.subject == "accounts.status").expect("accounts.status lifecycle");
    assert_eq!(sm.states.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["OPEN", "FROZEN", "CLOSED"]);
    assert!(sm.states[0].initial, "`.default(AccountStatus.OPEN)`");
    let to: Vec<&str> = sm.transitions.iter().map(|t| t.to.as_str()).collect();
    assert!(to.contains(&"FROZEN") && to.contains(&"CLOSED"), "{to:?}");
}

#[test]
fn spring_webflux_corouter_dsl_contracts() {
    let (root, r) = report("kotlin/webflux-corouter");
    let api = r.api.expect("api model");
    assert_api_evidence(&root, &api);
    let ids: Vec<&str> = api.operations.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "webflux-corouter:GET /api/books",
            "webflux-corouter:POST /api/books",
            "webflux-corouter:GET /api/books/{id}",
        ],
        "`\"/books\".nest {{ }}` nests under `spring.webflux.base-path`"
    );
    let op = |id: &str| api.operations.iter().find(|o| o.id == id).unwrap_or_else(|| panic!("no {id}"));

    // The route is cited where it is declared; the handler where it is written.
    let all = op("webflux-corouter:GET /api/books");
    assert_eq!(all.evidence.file_path, "src/main/kotlin/com/acme/catalog/Routes.kt");
    cites(&root, &all.evidence, &["GET(\"\", handler::all)"]);
    assert_eq!(all.handler.name, "all");
    assert_eq!(all.handler.evidence.file_path, "src/main/kotlin/com/acme/catalog/BookHandler.kt");
    assert_eq!(
        all.params.iter().map(|p| (p.name.as_str(), p.location.as_str(), p.required)).collect::<Vec<_>>(),
        vec![("author", "query", false)],
        "`request.queryParamOrNull(\"author\")`"
    );

    let by_id = op("webflux-corouter:GET /api/books/{id}");
    assert_eq!(
        by_id.params.iter().map(|p| (p.name.as_str(), p.location.as_str(), p.required)).collect::<Vec<_>>(),
        vec![("id", "path", true)],
        "`request.pathVariable(\"id\")`"
    );
    let err = by_id.errors.first().expect("ResponseStatusException in the handler");
    assert_eq!((err.status, err.message.as_deref()), (Some(404), Some("no such book")));

    let create = op("webflux-corouter:POST /api/books");
    assert_eq!(create.success_status, Some(201), "`ServerResponse.status(HttpStatus.CREATED)`");
    assert_eq!(
        create.request_body.as_ref().and_then(|b| b.model.as_deref()),
        Some("webflux-corouter:NewBook"),
        "`request.awaitBody<NewBook>()`"
    );
    let body = api.models.iter().find(|m| m.id == "webflux-corouter:NewBook").expect("NewBook");
    assert_eq!(
        body.fields.iter().map(|f| (f.name.as_str(), f.rules.len())).collect::<Vec<_>>(),
        vec![("title", 1), ("author", 1), ("pages", 1)],
        "`@field:` Bean Validation on the data class"
    );
}

#[test]
fn kotlin_clients_call_other_services_and_expect_their_types() {
    let root = fixture("kotlin/kotlin-modules");
    let report = scan(&root, &ScanOptions { behavior: true, ..Default::default() }).unwrap();
    let api = report.api.as_ref().expect("api model");
    let calls: Vec<&autodoc_analyzer::api::ClientCall> = api.client_calls.iter().filter(|c| c.unit == "app").collect();

    let feign = calls
        .iter()
        .find(|c| c.caller.as_deref() == Some("CatalogClient.item"))
        .unwrap_or_else(|| panic!("Feign call, have {calls:#?}"));
    assert_eq!((feign.method.as_str(), feign.path.as_str()), ("GET", "/catalog/{sku}"));
    assert_eq!(
        feign.target_unit.as_deref(),
        Some("catalog"),
        "`catalog-service` is the catalog unit's application name"
    );
    assert_eq!(feign.operation.as_deref(), Some("catalog:GET /catalog/{sku}"), "matched to the operation it calls");

    let ktor = calls
        .iter()
        .find(|c| c.evidence.file_path.ends_with("CatalogFeed.kt"))
        .unwrap_or_else(|| panic!("Ktor client call, have {calls:#?}"));
    assert_eq!(
        (ktor.method.as_str(), ktor.path.as_str(), ktor.target_unit.as_deref()),
        ("GET", "/catalog/all", Some("catalog"))
    );

    for c in [feign, ktor] {
        let text = std::fs::read_to_string(root.join(&c.evidence.file_path)).unwrap();
        let line = text.lines().nth(c.evidence.start_line as usize - 1).unwrap();
        assert!(line.contains("catalog") || line.contains("GetMapping"), "cited line names the call: {line}");
    }
    // The Ktor caller reads a `CatalogItem` with a `stock` field the service never returns.
    let drift = calls.iter().find(|c| !c.drift.is_empty()).unwrap_or_else(|| panic!("drift, have {calls:#?}"));
    assert_eq!(drift.drift, vec!["stock".to_string()]);
}
