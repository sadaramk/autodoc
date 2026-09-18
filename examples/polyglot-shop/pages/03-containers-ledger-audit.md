# Ledger Audit

_Container_ · [Book index](../README.md)

Read-only reconciliation reports over the payments ledger

**3** Files · **89** Lines · **3** Modules · **1** Depends on · **0** Used by

## At a glance

|   |   |
|---|---|
| Role | HTTP service |
| Technology | `Rust · Axum` |
| Path | `ledger-audit/` |
| Frameworks | Axum |
| Manifest | `ledger-audit/Cargo.toml` |
| Compose service | `ledger-audit` |
| Entry points | binary `fn main`  `ledger-audit/src/main.rs:10-21` |

## Components

![Ledger Audit — components](../diagrams/components-ledger-audit.svg)

_Components of Ledger Audit: modules and the imports between them._ · [IR](../diagrams/components-ledger-audit.ir.json)

## Modules

| Module | Path | Responsibility | Symbols | Evidence |
|---|---|---|---|---|
| **Main** _entry_ | `ledger-audit/src/main.rs` | ledger-audit: serves reconciliation reports over the payments table. | 3 | `ledger-audit/src/main.rs:10-21` |
| **Routes** | `ledger-audit/src/routes.rs` | HTTP routes for the audit service. | 5 | `ledger-audit/src/routes.rs:15-17` |
| **Audit** | `ledger-audit/src/audit.rs` | Reconciliation queries over the payments ledger. | 2 | `ledger-audit/src/audit.rs:8-12` |

## Depends on

| Target | Interaction | Basis | Evidence |
|---|---|---|---|
| **PostgreSQL** | reads payments | _observed in code_ | `ledger-audit/src/audit.rs:15-22` `ledger-audit/src/audit.rs:4` |

## Key symbols

| Symbol | Kind | Description | Evidence |
|---|---|---|---|
| `DailyTotal` | struct | Daily totals grouped by payment status. | `ledger-audit/src/audit.rs:8-12` |
| `DailyQuery` | struct | Filters for the daily report. | `ledger-audit/src/routes.rs:29-33` |
| `daily_totals` | function | Summarises today's payments by status. Read-only. | `ledger-audit/src/audit.rs:15-22` |
| `router` | function | Builds the router with shared database state. | `ledger-audit/src/routes.rs:15-17` |

