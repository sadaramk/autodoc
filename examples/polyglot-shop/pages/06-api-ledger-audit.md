# Ledger Audit API

_API reference_ · [Book index](../README.md)

The contract of every axum operation Ledger Audit serves: parameters with their wire names and rules, request and response models, errors, authentication and who calls it.

**2** operations · **2** typed contracts · **0** partial · **0** opaque

> [!NOTE]
> **Contract coverage** — 2 of 2 operations declare request and response types; 0 partially; 0 not at all (the handler reads the request at runtime).

## Operations

| Method | Path | Request | Response | Auth | Contract |
|---|---|---|---|---|---|
| `GET` | [/reports/daily](../pages/06-api-ledger-audit.md#op-get-reports-daily) | — | [Vec&lt;DailyTotal>[]](../pages/06-api-ledger-audit.md#model-dailytotal) | _none found_ | _typed_ |
| `GET` | [/reports/daily/{status}](../pages/06-api-ledger-audit.md#op-get-reports-daily-status) | — | [DailyTotal](../pages/06-api-ledger-audit.md#model-dailytotal) | _none found_ | _typed_ |

## GET /reports/daily

GET /reports/daily — today's payment totals by status. Handled by `daily_report` [`ledger-audit/src/routes.rs:36-40`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L36-L40) · _typed_

| Parameter | In | Type | Required | Rules |
|---|---|---|---|---|
| `minCount` (code: min\_count) [`ledger-audit/src/routes.rs:32`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L32) | query | `Option<i64>` | no | — |

- **Response 200** [Vec&lt;DailyTotal>[]](../pages/06-api-ledger-audit.md#model-dailytotal)

Behaviour: [sequence diagram](../pages/05-critical-flows.md#flow-ledger-audit-get-reports-daily)

## GET /reports/daily/{status}

GET /reports/daily/{status} — today's total for one payment status. Handled by `daily_status` [`ledger-audit/src/routes.rs:43-46`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L43-L46) · _typed_

| Parameter | In | Type | Required | Rules |
|---|---|---|---|---|
| `status` [`ledger-audit/src/routes.rs:43`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L43) | path | `String` | yes | — |

- **Response 200** [DailyTotal](../pages/06-api-ledger-audit.md#model-dailytotal)

| Error | Message | Raised at |
|---|---|---|
| `404` | — | [`ledger-audit/src/routes.rs:45`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L45) |
| `503` | — | [`ledger-audit/src/routes.rs:44`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L44) |

Behaviour: [sequence diagram](../pages/05-critical-flows.md#flow-ledger-audit-get-reports-daily-status)

## Models

### DailyTotal

Daily totals grouped by payment status. Declared [`ledger-audit/src/audit.rs:8-12`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L8-L12)

| DailyTotal field | Type | Required | Rules |
|---|---|---|---|
| `status` [`ledger-audit/src/audit.rs:9`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L9) | `String` | yes | — |
| `count` [`ledger-audit/src/audit.rs:10`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L10) | `i64` | yes | — |
| `amount_cents` [`ledger-audit/src/audit.rs:11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L11) | `i64` | yes | — |

