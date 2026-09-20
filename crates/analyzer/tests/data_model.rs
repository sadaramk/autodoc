//! Integration: entities, relations, data access and state machines extracted
//! from fixture repositories. Every evidence line is read back and must
//! mention what it is cited for.

use std::path::{Path, PathBuf};

use autodoc_analyzer::data::{Access, Column, DataModel, Entity, StateMachine};
use autodoc_analyzer::{scan, EvidenceRef, ScanOptions};

fn fixture(name: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(name)
}

fn model(root: &Path) -> DataModel {
    scan(root, &ScanOptions { behavior: true, ..Default::default() })
        .unwrap()
        .data
        .expect("behavior scan returns a data model")
}

fn entity<'a>(m: &'a DataModel, table: &str) -> &'a Entity {
    m.entities.iter().find(|e| e.table == table).unwrap_or_else(|| panic!("entity {table} missing: {:?}", tables(m)))
}

fn tables(m: &DataModel) -> Vec<&str> {
    m.entities.iter().map(|e| e.table.as_str()).collect()
}

fn col<'a>(e: &'a Entity, name: &str) -> &'a Column {
    e.columns.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{}.{name} missing", e.table))
}

fn machine<'a>(m: &'a DataModel, id: &str) -> &'a StateMachine {
    m.state_machines
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("{id} missing: {:?}", m.state_machines.iter().map(|s| &s.id).collect::<Vec<_>>()))
}

fn norm(s: &str) -> String {
    s.to_lowercase().replace(['_', '-'], "")
}

/// The cited line exists and contains one of `needles` (case / underscore insensitive).
fn cites(root: &Path, e: &EvidenceRef, needles: &[&str]) {
    let text = std::fs::read_to_string(root.join(&e.file_path)).unwrap_or_else(|_| panic!("{} missing", e.file_path));
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        e.start_line >= 1 && e.start_line <= e.end_line && e.end_line as usize <= lines.len(),
        "{e:?} out of range"
    );
    let window = norm(&lines[e.start_line as usize - 1..e.end_line as usize].join("\n"));
    assert!(
        needles.iter().any(|n| window.contains(&norm(n))),
        "{}:{} does not mention any of {needles:?}",
        e.file_path,
        e.start_line
    );
}

fn sites(a: &[Access]) -> Vec<(&str, Option<&str>, &str, u32)> {
    let mut v: Vec<_> = a
        .iter()
        .map(|x| (x.unit.as_str(), x.symbol.as_deref(), x.evidence.file_path.as_str(), x.evidence.start_line))
        .collect();
    v.sort();
    v
}

/// Every piece of evidence in the model points at a line naming the thing.
fn assert_model_evidence(root: &Path, m: &DataModel) {
    for e in &m.entities {
        cites(root, &e.evidence, &[&e.name, &e.table]);
        for c in &e.columns {
            // Django `author = ForeignKey(…)` declares `author_id`; `gorm.Model` / `models.Model` / Panache declare the id.
            cites(
                root,
                &c.evidence,
                &[&c.name, c.name.trim_end_matches("_id"), "gorm.Model", "models.Model", "PanacheEntity"],
            );
        }
        for r in &e.relations {
            let target = m.entities.iter().find(|t| t.id == r.target).expect("relation targets an extracted entity");
            cites(root, &r.evidence, &[&r.via, &target.table, &target.name]);
        }
        let mut names = vec![e.name.as_str(), e.table.as_str()];
        names.extend(e.columns.iter().map(|c| c.name.as_str()));
        for a in e.reads.iter().chain(&e.writes) {
            cites(root, &a.evidence, &names);
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

#[test]
fn polyglot_shop_tables_from_migrations() {
    let root = fixture("polyglot-shop");
    let m = model(&root);
    assert_eq!(tables(&m), vec!["order_items", "orders", "payments", "products"]);

    let orders = entity(&m, "orders");
    assert_eq!(
        (orders.source.as_str(), orders.evidence.file_path.as_str(), orders.evidence.start_line),
        ("sql-ddl", "db/migrations/001_init.sql", 10)
    );
    let names: Vec<&str> = orders.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "total_cents", "status", "items", "tracking_number", "created_at"]);
    let id = col(orders, "id");
    assert!(id.primary_key && !id.nullable && id.unique);
    let status = col(orders, "status");
    assert_eq!(
        (status.type_name.as_str(), status.nullable, status.default.as_deref()),
        ("TEXT", false, Some("pending"))
    );
    assert!(col(orders, "tracking_number").nullable);
    assert_eq!(col(orders, "total_cents").constraints, vec!["CHECK (total_cents > 0)"]);

    let items = entity(&m, "order_items");
    assert!(col(items, "order_id").primary_key && col(items, "sku").primary_key, "composite key");
    assert!(!col(items, "order_id").unique, "one column of a composite key is not unique alone");
    assert_eq!(col(items, "order_id").references.as_deref(), Some("orders.id"));
    assert_eq!(col(items, "sku").references.as_deref(), Some("products.sku"));
    let rels: Vec<(&str, &str, &str)> =
        items.relations.iter().map(|r| (r.kind.as_str(), r.target.as_str(), r.via.as_str())).collect();
    assert_eq!(rels, vec![("many-to-one", "entity:orders", "order_id"), ("many-to-one", "entity:products", "sku")]);

    // ALTER TABLE in the second migration adds the FK and the unique charge id.
    let payments = entity(&m, "payments");
    assert_eq!(col(payments, "order_id").references.as_deref(), Some("orders.id"));
    assert!(col(payments, "charge_id").unique);
    assert_eq!(payments.relations[0].evidence.file_path, "db/migrations/002_payments.sql");
    assert_eq!(payments.relations[0].evidence.start_line, 13);

    assert_model_evidence(&root, &m);
}

#[test]
fn polyglot_shop_reads_and_writes_by_unit() {
    let m = model(&fixture("polyglot-shop"));
    assert_eq!(
        sites(&entity(&m, "orders").writes),
        vec![
            ("api-gateway", Some("cancelOrder"), "api-gateway/src/db.ts", 35),
            ("api-gateway", Some("insertOrder"), "api-gateway/src/db.ts", 16),
            ("api-gateway", Some("markOrderPaid"), "api-gateway/src/db.ts", 30),
            ("fulfillment", Some("ship_order"), "fulfillment/fulfillment/shipping.py", 14),
        ]
    );
    assert!(entity(&m, "orders").reads.is_empty());
    assert_eq!(
        sites(&entity(&m, "products").reads),
        vec![("api-gateway", Some("listProducts"), "api-gateway/src/db.ts", 24)]
    );
    assert_eq!(
        sites(&entity(&m, "payments").writes),
        vec![("payments", Some("Record"), "payments/internal/ledger/ledger.go", 23)]
    );
    assert_eq!(
        sites(&entity(&m, "payments").reads),
        vec![("ledger-audit", Some("daily_totals"), "ledger-audit/src/audit.rs", 17)]
    );
    assert_eq!(entity(&m, "orders").units, vec!["api-gateway", "fulfillment"]);
}

#[test]
fn polyglot_shop_order_lifecycle() {
    let m = model(&fixture("polyglot-shop"));
    // payments.status is a closed set too, but no code moves it between values: no machine.
    assert_eq!(m.state_machines.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["state:orders.status"]);
    let sm = machine(&m, "state:orders.status");
    assert_eq!(sm.subject, "orders.status");
    let states: Vec<(&str, bool, bool, u32)> =
        sm.states.iter().map(|s| (s.name.as_str(), s.initial, s.terminal, s.evidence.start_line)).collect();
    assert_eq!(
        states,
        vec![
            ("pending", true, false, 13),
            ("paid", false, false, 14),
            ("shipped", false, true, 14),
            ("cancelled", false, true, 14)
        ]
    );
    type Row<'a> = (Option<&'a str>, &'a str, &'a str, Option<&'a str>, Option<&'a str>, u32);
    let mut t: Vec<Row> = sm
        .transitions
        .iter()
        .map(|t| {
            (
                t.from.as_deref(),
                t.to.as_str(),
                t.unit.as_str(),
                t.trigger.as_deref(),
                t.guard.as_deref(),
                t.evidence.start_line,
            )
        })
        .collect();
    t.sort();
    assert_eq!(
        t,
        vec![
            (Some("paid"), "shipped", "fulfillment", Some("ship_order"), None, 14),
            (Some("pending"), "cancelled", "api-gateway", Some("cancelOrder"), None, 35),
            (Some("pending"), "paid", "api-gateway", Some("markOrderPaid"), None, 30),
        ]
    );
}

#[test]
fn sqlmodel_entities_with_validation_and_crud_access() {
    let root = fixture("real-world/uv-workspace");
    let m = model(&root);
    assert_eq!(tables(&m), vec!["item", "user"]);
    let user = entity(&m, "user");
    assert_eq!((user.name.as_str(), user.source.as_str()), ("User", "sqlmodel"));
    let names: Vec<&str> = user.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["id", "email", "is_active", "full_name", "hashed_password"],
        "inherited base fields included, key first"
    );
    assert!(col(user, "email").unique && col(user, "email").constraints == vec!["max length 255"]);
    assert!(col(user, "full_name").nullable && !col(user, "email").nullable);
    assert_eq!(col(user, "is_active").default.as_deref(), Some("True"));
    assert_eq!(
        user.relations.iter().map(|r| (r.kind.as_str(), r.target.as_str())).collect::<Vec<_>>(),
        vec![("one-to-many", "entity:item")]
    );

    let item = entity(&m, "item");
    let owner = col(item, "owner_id");
    assert_eq!((owner.references.as_deref(), owner.nullable), (Some("user.id"), false));
    assert_eq!(col(item, "title").constraints, vec!["max length 255", "min length 1"]);
    assert_eq!(item.relations.len(), 1, "the owner relationship is the owner_id FK, not a second edge");
    assert_eq!(sites(&item.reads), vec![("backend", Some("list_items"), "backend/app/crud.py", 12)]);
    assert_eq!(sites(&item.writes), vec![("backend", Some("create_item"), "backend/app/crud.py", 18)]);
    // The Alembic `UPDATE item` is a migration, not runtime access.
    assert!(item.writes.iter().all(|w| !w.evidence.file_path.contains("alembic")));
    assert!(m.state_machines.is_empty());
    assert_model_evidence(&root, &m);
}

#[test]
fn orm_models_across_frameworks() {
    let root = fixture("orm-models");
    let m = model(&root);
    let summary: Vec<(&str, &str, &str)> =
        m.entities.iter().map(|e| (e.table.as_str(), e.name.as_str(), e.source.as_str())).collect();
    assert_eq!(
        summary,
        vec![
            ("accounts", "Account", "sqlalchemy"),
            ("appointments", "Appointment", "jpa"),
            // One service, two stores: the write must reach the right one.
            ("audit_events", "AuditEvent", "spring-data-mongodb"),
            ("audit_invoices", "AuditInvoice", "jpa"),
            ("carts", "Cart", "spring-data-mongodb"),
            ("clinics", "Clinic", "sql-ddl+jpa"),
            ("customers", "customers", "drizzle"),
            ("customers_go", "Customer", "gorm"),
            ("device_events", "DeviceEvent", "jpa"),
            ("devices", "Device", "jpa"),
            ("invoice_lines", "InvoiceLine", "typeorm"),
            ("invoices", "Invoice", "typeorm"),
            ("ledger_entries", "LedgerEntry", "liquibase+jpa"),
            ("ledgers", "ledgers", "liquibase"),
            ("orders", "Order", "gorm"),
            ("owners", "Owner", "sqlalchemy"),
            ("posts", "Post", "prisma"),
            ("projects", "projects", "diesel"),
            ("shop_author", "Author", "django"),
            ("shop_book", "Book", "django"),
            ("specialties", "Specialty", "jpa"),
            // A drizzle table whose builder chains are wrapped by Prettier.
            ("statements", "statements", "drizzle"),
            ("subscriptions", "subscriptions", "drizzle"),
            ("tasks", "Task", "diesel+diesel-model"),
            ("users", "User", "prisma"),
            ("vet", "Vet", "jpa"),
            ("voucher", "Voucher", "spring-data-mongodb"),
        ]
    );
    let refs = |t: &str, c: &str| col(entity(&m, t), c).references.clone();

    // Prisma: @@map, @map, @relation(fields, references), VarChar length, enum default.
    let posts = entity(&m, "posts");
    assert_eq!(refs("posts", "author_id").as_deref(), Some("users.id"));
    assert_eq!(col(posts, "status").default.as_deref(), Some("DRAFT"));
    assert_eq!(col(entity(&m, "users"), "email").constraints, vec!["max length 255"]);
    assert!(col(entity(&m, "users"), "name").nullable);
    // SQLAlchemy 2.0: mapped_column, Mapped[Optional], String(n), ForeignKey.
    assert_eq!(refs("accounts", "owner_id").as_deref(), Some("owners.id"));
    assert!(col(entity(&m, "accounts"), "iban").nullable);
    assert_eq!(col(entity(&m, "owners"), "name").constraints, vec!["max length 100"]);
    // Django: implicit id, ForeignKey → author_id, app-prefixed table names.
    let book = entity(&m, "shop_book");
    assert!(col(book, "id").primary_key);
    assert_eq!(refs("shop_book", "author_id").as_deref(), Some("shop_author.id"));
    assert!(col(book, "title").unique);
    // TypeORM: @Entity(name), @JoinColumn, column options.
    assert_eq!(refs("invoice_lines", "invoice_id").as_deref(), Some("invoices.id"));
    let invoices = entity(&m, "invoices");
    assert!(col(invoices, "number").unique && col(invoices, "note").nullable);
    // Drizzle: .primaryKey / .notNull / .references(() => customers.id).
    assert_eq!(refs("subscriptions", "customer_id").as_deref(), Some("customers.id"));
    assert!(!col(entity(&m, "customers"), "email").nullable && col(entity(&m, "customers"), "email").unique);
    // GORM: gorm.Model, TableName(), belongs-to by field, size / check tags, pointer = nullable.
    let orders = entity(&m, "orders");
    assert_eq!(
        orders.columns.iter().take(4).map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "created_at", "updated_at", "deleted_at"]
    );
    assert_eq!(refs("orders", "customer_id").as_deref(), Some("customers_go.id"));
    assert_eq!(col(orders, "total").constraints, vec!["CHECK (total > 0)"]);
    assert!(col(orders, "note").nullable);
    // Diesel: table! + joinable!, Nullable<>, model struct supplies the code name.
    assert_eq!(refs("tasks", "project_id").as_deref(), Some("projects.id"));
    assert!(col(entity(&m, "projects"), "archived_at").nullable);

    // Access through each ORM.
    let w = |t: &str| sites(&entity(&m, t).writes).into_iter().map(|s| s.1.unwrap_or("")).collect::<Vec<_>>();
    let r = |t: &str| sites(&entity(&m, t).reads).into_iter().map(|s| s.1.unwrap_or("")).collect::<Vec<_>>();
    assert_eq!(w("posts"), vec!["archivePost", "publishPost"]);
    assert_eq!(r("posts"), vec!["postsBy"]);
    assert_eq!(w("invoices"), vec!["markInvoicePaid", "sendInvoice"]);
    assert_eq!(w("subscriptions"), vec!["subscribe"]);
    assert_eq!(r("customers"), vec!["listCustomers"]);
    assert_eq!(r("accounts"), vec!["accounts_for"]);
    assert_eq!((r("shop_book"), w("shop_book")), (vec!["publish"], vec!["publish"]));
    assert_eq!((r("orders"), w("orders")), (vec!["Refund"], vec!["Pay", "Place", "Refund"]));
    assert_eq!((r("tasks"), w("tasks")), (vec![], vec!["finish_task"]), "a chained diesel update is one write");

    assert_model_evidence(&root, &m);
}

#[test]
fn orm_models_state_machines() {
    let m = model(&fixture("orm-models"));
    let ids: Vec<&str> = m.state_machines.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "state:ShipmentState",
            "state:TicketStatus",
            "state:appointments.status",
            "state:devices.state",
            "state:invoices.status",
            "state:orders.status",
            "state:posts.status",
            "state:shop_book.status",
            "state:tasks.state",
        ]
    );
    let flow = |id: &str| {
        let sm = machine(&m, id);
        let states: Vec<String> = sm
            .states
            .iter()
            .map(|s| format!("{}{}{}", s.name, if s.initial { "*" } else { "" }, if s.terminal { "!" } else { "" }))
            .collect();
        let mut t: Vec<String> = sm
            .transitions
            .iter()
            .map(|t| format!("{}->{}@{}", t.from.as_deref().unwrap_or("?"), t.to, t.trigger.as_deref().unwrap_or("")))
            .collect();
        t.sort();
        (states.join(" "), t.join(", "))
    };
    // Prisma enum + where/data: the where clause gives the source state.
    assert_eq!(
        flow("state:posts.status"),
        ("DRAFT* PUBLISHED ARCHIVED!".into(), "?->ARCHIVED@archivePost, DRAFT->PUBLISHED@publishPost".into())
    );
    // TypeORM enum: a thrown guard before the assignment, and update(criteria, values).
    assert_eq!(
        flow("state:invoices.status"),
        ("draft* sent paid!".into(), "draft->sent@sendInvoice, sent->paid@markInvoicePaid".into())
    );
    // Django TextChoices.
    assert_eq!(flow("state:shop_book.status"), ("draft* published!".into(), "draft->published@publish".into()));
    // GORM typed string consts: Where("… status = ?", OrderNew).Update("status", …) and field assignment.
    assert_eq!(
        flow("state:orders.status"),
        ("new* paid refunded!".into(), "new->paid@Pay, paid->refunded@Refund".into())
    );
    // Diesel .filter(state.eq(..)).set(state.eq(..)) with a serde-renamed Rust enum.
    assert_eq!(flow("state:tasks.state"), ("todo doing done!".into(), "doing->done@finish_task".into()));
    // Code-only lifecycles: Python Enum on a dataclass, TypeScript string union.
    assert_eq!(
        flow("state:TicketStatus"),
        ("open* in_progress closed!".into(), "?->closed@close, open->in_progress@start".into())
    );
    assert_eq!(
        flow("state:ShipmentState"),
        ("created in_transit delivered!".into(), "in_transit->delivered@confirmDelivery".into())
    );
    let guards: Vec<&str> =
        m.state_machines.iter().flat_map(|s| &s.transitions).filter_map(|t| t.guard.as_deref()).collect();
    assert_eq!(
        guards,
        vec!["a.createdAt < :cutoff"],
        "guards restating the source state are dropped; business conditions stay"
    );
}

#[test]
fn jvm_persistence_across_frameworks() {
    let root = fixture("orm-models");
    let m = model(&root);
    fn names(e: &Entity) -> Vec<&str> {
        e.columns.iter().map(|c| c.name.as_str()).collect()
    }

    // JPA with Spring Boot naming: @MappedSuperclass fields first, @JoinColumn, @Column(name), @Transient skipped.
    let appt = entity(&m, "appointments");
    assert_eq!(
        names(appt),
        vec!["id", "created_at", "vet_id", "pet_name", "reason", "duration_minutes", "fee", "status"]
    );
    assert_eq!(appt.columns[0].evidence.file_path, "jpa-spring/src/main/java/com/acme/clinic/domain/BaseEntity.java");
    let id = col(appt, "id");
    assert!(id.primary_key && !id.nullable && id.default.as_deref() == Some("generated"));
    let vet_id = col(appt, "vet_id");
    assert_eq!((vet_id.references.as_deref(), vet_id.nullable), (Some("vet.id"), false), "optional = false");
    let pet = col(appt, "pet_name");
    assert_eq!(
        (pet.type_name.as_str(), pet.nullable, pet.constraints.clone()),
        ("varchar(60)", false, vec!["max length 60".to_string()])
    );
    assert_eq!(col(appt, "reason").constraints, vec!["max length 500"]);
    assert!(!col(appt, "reason").nullable, "@NotNull");
    assert_eq!(col(appt, "duration_minutes").constraints, vec!["≥ 15", "≤ 120"]);
    assert_eq!(col(appt, "fee").type_name, "numeric(10,2)");
    assert_eq!(col(appt, "status").default.as_deref(), Some("REQUESTED"));

    // Default table name (snake_case class), many-to-one without @JoinColumn, @JoinTable, inverse side.
    let vet = entity(&m, "vet");
    assert_eq!(col(vet, "clinic_id").references.as_deref(), Some("clinics.id"));
    let rels: Vec<(&str, &str, &str)> =
        vet.relations.iter().map(|r| (r.kind.as_str(), r.target.as_str(), r.via.as_str())).collect();
    assert_eq!(
        rels,
        vec![
            ("many-to-one", "entity:clinics", "clinic_id"),
            ("many-to-many", "entity:specialties", "vet_specialties"),
            ("one-to-many", "entity:appointments", "vet"),
        ]
    );
    assert!(col(entity(&m, "specialties"), "label").unique, "single-column @UniqueConstraint");

    // Flyway DDL supplies physical columns; the entity adds validation; @Embedded flattens.
    let clinics = entity(&m, "clinics");
    assert_eq!(names(clinics), vec!["id", "name", "street", "city"]);
    assert_eq!(col(clinics, "name").type_name, "VARCHAR(120)");
    assert_eq!(col(clinics, "name").constraints, vec!["not blank"]);

    // Spring Data MongoDB: @Document(collection), @Field, @Indexed(unique), @DBRef, default collection name.
    let carts = entity(&m, "carts");
    assert_eq!(names(carts), vec!["id", "customer_ref", "sessionKey", "lines", "voucher"]);
    assert!(col(carts, "sessionKey").unique);
    assert_eq!(col(carts, "voucher").references.as_deref(), Some("voucher.code"));

    // Quarkus Panache: implicit id, explicit @Id on PanacheEntityBase.
    assert_eq!(names(entity(&m, "devices")), vec!["id", "serial", "state"]);
    assert!(col(entity(&m, "device_events"), "eventId").primary_key, "plain Hibernate keeps field names");
    assert_eq!(col(entity(&m, "device_events"), "device_id").references.as_deref(), Some("devices.id"));

    // Liquibase: YAML include order, renameColumn then addForeignKeyConstraint; merged with a plain JPA entity.
    let entries = entity(&m, "ledger_entries");
    assert_eq!(names(entries), vec!["id", "ledger_id", "amount_cents"]);
    assert_eq!(col(entries, "ledger_id").references.as_deref(), Some("ledgers.id"));
    assert_eq!(col(entries, "amount_cents").constraints, vec!["> 0"]);
    assert!(col(entity(&m, "ledgers"), "code").unique);

    // Access: repository fields (constructor / Lombok injection), custom @Query methods, Panache, EntityManager, MongoTemplate.
    let w = |t: &str| sites(&entity(&m, t).writes).into_iter().map(|s| s.1.unwrap_or("")).collect::<Vec<_>>();
    let r = |t: &str| sites(&entity(&m, t).reads).into_iter().map(|s| s.1.unwrap_or("")).collect::<Vec<_>>();
    let w_appt = w("appointments");
    assert!(w_appt.contains(&"book") && w_appt.contains(&"cleanup"), "{w_appt:?}");
    let r_appt = r("appointments");
    for f in ["confirm", "complete", "cancel"] {
        assert!(r_appt.contains(&f), "{r_appt:?}");
    }
    assert_eq!(r("vet"), vec!["book"]);
    assert_eq!((r("carts"), w("carts")), (vec!["open"], vec!["open", "purgeAbandoned"]));
    assert_eq!(r("voucher"), vec!["voucher"]);
    assert_eq!((r("devices"), w("devices")), (vec!["activate", "list", "retire"], vec!["retire"]));
    assert_eq!(w("device_events"), vec!["activate"]);
    assert_eq!((r("ledger_entries"), w("ledger_entries")), (vec!["recent"], vec!["record"]));

    // Lifecycles: setter with a thrown guard, getter .equals guard, unguarded cancel, bulk JPQL update; Panache field assignment.
    let flow = |id: &str| {
        let sm = machine(&m, id);
        let states: Vec<String> =
            sm.states.iter().map(|s| format!("{}{}", s.name, if s.initial { "*" } else { "" })).collect();
        let mut t: Vec<String> = sm
            .transitions
            .iter()
            .map(|t| format!("{}->{}@{}", t.from.as_deref().unwrap_or("?"), t.to, t.trigger.as_deref().unwrap_or("")))
            .collect();
        t.sort();
        (states.join(" "), t.join(", "))
    };
    assert_eq!(
        flow("state:appointments.status"),
        (
            "REQUESTED* CONFIRMED COMPLETED CANCELLED".into(),
            "?->CANCELLED@cancel, ?->CANCELLED@cancelStale, CONFIRMED->COMPLETED@complete, REQUESTED->CONFIRMED@confirm".into()
        )
    );
    assert_eq!(
        flow("state:devices.state"),
        ("PROVISIONED* ACTIVE RETIRED".into(), "?->RETIRED@retire, PROVISIONED->ACTIVE@activate".into())
    );
    assert_model_evidence(&root, &m);
}

/// Drizzle columns were read one physical line at a time, so a builder chain
/// wrapped by Prettier — the default at 80 columns — lost everything after the
/// first line. The key was documented as an ordinary nullable column and the
/// foreign key disappeared.
#[test]
fn drizzle_columns_survive_a_wrapped_builder_chain() {
    let data = model(&fixture("orm-models/node-app"));
    let invoices = entity(&data, "statements");
    let col = |name: &str| col(invoices, name);

    assert!(col("id").primary_key, "`.primaryKey()` on the next line is still a primary key");
    assert!(!col("customer_id").nullable, "`.notNull()` on the next line still makes the column required");
    assert!(col("reference").unique, "`.unique()` on the next line is still a unique constraint");
    assert!(
        invoices.relations.iter().any(|r| r.via == "customer_id"),
        "the foreign key is still a relation: {:?}",
        invoices.relations
    );
}
