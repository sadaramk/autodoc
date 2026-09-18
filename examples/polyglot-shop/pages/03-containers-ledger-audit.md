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
| Entry points | binary `fn main`  [`ledger-audit/src/main.rs:10-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/main.rs#L10-L21) |

## Components

![Ledger Audit — components](../diagrams/components-ledger-audit.svg)

_Components of Ledger Audit: modules and the imports between them._ · [IR](../diagrams/components-ledger-audit.ir.json)

## Modules

| Module | Path | Responsibility | Symbols | Evidence |
|---|---|---|---|---|
| **Main** _entry_ | `ledger-audit/src/main.rs` | ledger-audit: serves reconciliation reports over the payments table. | 3 | [`ledger-audit/src/main.rs:10-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/main.rs#L10-L21) |
| **Routes** | `ledger-audit/src/routes.rs` | HTTP routes for the audit service. | 5 | [`ledger-audit/src/routes.rs:15-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L15-L17) |
| **Audit** | `ledger-audit/src/audit.rs` | Reconciliation queries over the payments ledger. | 2 | [`ledger-audit/src/audit.rs:8-12`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L8-L12) |

## Depends on

| Target | Interaction | Basis | Evidence |
|---|---|---|---|
| **PostgreSQL** | reads payments | _observed in code_ | [`ledger-audit/src/audit.rs:15-22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L15-L22) [`ledger-audit/src/audit.rs:4`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L4) |

## Key symbols

| Symbol | Kind | Description | Evidence |
|---|---|---|---|
| `DailyTotal` | struct | Daily totals grouped by payment status. | [`ledger-audit/src/audit.rs:8-12`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L8-L12) |
| `DailyQuery` | struct | Filters for the daily report. | [`ledger-audit/src/routes.rs:29-33`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L29-L33) |
| `daily_totals` | function | Summarises today's payments by status. Read-only. | [`ledger-audit/src/audit.rs:15-22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L15-L22) |
| `router` | function | Builds the router with shared database state. | [`ledger-audit/src/routes.rs:15-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L15-L17) |

