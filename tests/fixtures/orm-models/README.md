# orm-models

Data-model extraction fixture: one small app per ORM. Tables are distinct across apps so each
entity has exactly one declaring source.

| App | Declares | State machine |
|-----|----------|---------------|
| `node-app/prisma/schema.prisma` | `users`, `posts` (FK `author_id`), enum `PostStatus` | `posts.status`: DRAFT → PUBLISHED (guarded), → ARCHIVED |
| `node-app/src/typeorm` | `invoices`, `invoice_lines` (`@ManyToOne`) | `invoices.status`: draft → sent → paid |
| `node-app/src/drizzle` | `customers`, `subscriptions` (`.references`) | — |
| `node-app/src/services/shipments.ts` | TS union `ShipmentState` | in_transit → delivered |
| `py-app/app/sa_models.py` | SQLAlchemy 2 `owners`, `accounts` | — |
| `py-app/shop/models.py` | Django `shop_author`, `shop_book`, `BookStatus` choices | `shop_book.status`: draft → published |
| `py-app/app/tickets.py` | Python `TicketStatus` enum on a dataclass | open → in_progress, → closed |
| `go-app/models` | GORM `customers_go`, `orders` + `OrderStatus` consts | `orders.status`: new → paid → refunded |
| `rust-app/src/schema.rs` | Diesel `projects`, `tasks`, `joinable!` + `Task` model, `TaskState` | `tasks.state`: doing → done |
| `jpa-spring` (Maven, Spring Boot) | JPA `appointments`, `vet` (default snake_case name), `specialties`, `clinics` (+ Flyway `V1__clinics.sql`), `@MappedSuperclass`, `@Embedded`, `@ManyToMany @JoinTable` | `appointments.status`: REQUESTED → CONFIRMED (thrown guard) → COMPLETED (`.equals` guard), → CANCELLED (unguarded, and a bulk `@Modifying @Query`) |
| `mongo-spring` (Maven) | Spring Data MongoDB `carts` (`@Field`, `@Indexed(unique)`, `@DBRef`), `voucher` (default collection name); `MongoRepository` + `MongoTemplate` access | — |
| `quarkus-panache` (Gradle) | Panache `devices` (implicit `id`), `device_events` (`PanacheEntityBase`, `PanacheRepository`) | `devices.state`: PROVISIONED → ACTIVE (guarded), → RETIRED |
| `liquibase` (Maven) | Liquibase master YAML → XML + YAML changesets: `ledgers`, `ledger_entries` (renameColumn, addForeignKeyConstraint) merged with a plain-Hibernate `LedgerEntry` and `EntityManager` access | — |
