# Data & integrations

_Data_ · [Book index](../README.md)

Where state lives, who reads and writes it, which events flow where, and which vendors are called.

## Datastore access

Rows are the services that touch state; columns are the stores. Reads and writes come from SQL literals and cache commands; queries means an ORM or driver without literal SQL.

| Service | PostgreSQL | Redis |
|---|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | _writes orders_ [`api-gateway/src/db.ts:13-20`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L13-L20) | _reads & writes_ [`api-gateway/src/cache.ts:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/cache.ts#L9-L17) |
| [Fulfillment](../pages/03-containers-fulfillment.md) | _writes orders_ [`fulfillment/fulfillment/shipping.py:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L9-L17) | — |
| [Ledger Audit](../pages/03-containers-ledger-audit.md) | _reads payments_ [`ledger-audit/src/audit.rs:15-22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L15-L22) | — |
| [Payments](../pages/03-containers-payments.md) | _writes payments_ [`payments/internal/ledger/ledger.go:21-26`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/ledger/ledger.go#L21-L26) | — |

## Events

| Topic | Bus | Published by | Consumed by |
|---|---|---|---|
| `order.placed` | **Kafka** | [API Gateway](../pages/03-containers-api-gateway.md) [`api-gateway/src/events.ts:16-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/events.ts#L16-L21) | [Fulfillment](../pages/03-containers-fulfillment.md) [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29) |

## External services

| Service | Category | Used for | Called by |
|---|---|---|---|
| **Stripe** | Payments API | charges cards | [Payments](../pages/03-containers-payments.md) [`payments/internal/charge/charge.go:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/charge/charge.go#L5) |
| **SendGrid** | Email delivery | sends email | [Fulfillment](../pages/03-containers-fulfillment.md) [`fulfillment/fulfillment/notify.py:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/notify.py#L5) |

## Data model

4 persisted entities declared in sql-ddl. Keys, nullability and relationships are read from the declarations; crow's feet mark the many side.

![Data model](../diagrams/data-model.svg)

_Entities and relationships_ · [IR](../diagrams/data-model.ir.json)

| Entity | Declared in | Columns | Written by | Read by |
|---|---|---|---|---|
| [order\_items](../pages/04-data-and-integrations.md#entity-order-items)  [`db/migrations/001_init.sql:20`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L20) | sql-ddl | 4 | _none found_ | _none found_ |
| [orders](../pages/04-data-and-integrations.md#entity-orders)  [`db/migrations/001_init.sql:10`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L10) | sql-ddl | 6 | [API Gateway](../pages/03-containers-api-gateway.md) [`api-gateway/src/db.ts:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L16), [Fulfillment](../pages/03-containers-fulfillment.md) [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14) | _none found_ |
| [payments](../pages/04-data-and-integrations.md#entity-payments)  [`db/migrations/002_payments.sql:3`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L3) | sql-ddl | 6 | [Payments](../pages/03-containers-payments.md) [`payments/internal/ledger/ledger.go:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/ledger/ledger.go#L23) | [Ledger Audit](../pages/03-containers-ledger-audit.md) [`ledger-audit/src/audit.rs:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L17) |
| [products](../pages/04-data-and-integrations.md#entity-products)  [`db/migrations/001_init.sql:3`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L3) | sql-ddl | 4 | _none found_ | [API Gateway](../pages/03-containers-api-gateway.md) [`api-gateway/src/db.ts:24`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L24) |

### order_items

| Column | Type | Key | Nullable | Default & constraints |
|---|---|---|---|---|
| `order_id`  [`db/migrations/001_init.sql:21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L21) | `UUID` | _PK__FK_ → orders.id | no |  |
| `sku`  [`db/migrations/001_init.sql:22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L22) | `TEXT` | _PK__FK_ → products.sku | no |  |
| `quantity`  [`db/migrations/001_init.sql:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L23) | `INTEGER` |  | no | `CHECK (quantity > 0)` |
| `price_cents`  [`db/migrations/001_init.sql:24`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L24) | `INTEGER` |  | no |  |

### orders

| Column | Type | Key | Nullable | Default & constraints |
|---|---|---|---|---|
| `id`  [`db/migrations/001_init.sql:11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L11) | `UUID` | _PK__unique_ | no | default `gen_random_uuid()` |
| `total_cents`  [`db/migrations/001_init.sql:12`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L12) | `INTEGER` |  | no | `CHECK (total_cents > 0)` |
| `status`  [`db/migrations/001_init.sql:13`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L13) | `TEXT` |  | no | default `pending`; `CHECK (status IN ('pending', 'paid', 'shipped', 'cancelled'))` |
| `items`  [`db/migrations/001_init.sql:15`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L15) | `JSONB` |  | no |  |
| `tracking_number`  [`db/migrations/001_init.sql:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L16) | `TEXT` |  | yes |  |
| `created_at`  [`db/migrations/001_init.sql:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L17) | `TIMESTAMPTZ` |  | no | default `now()` |

### payments

| Column | Type | Key | Nullable | Default & constraints |
|---|---|---|---|---|
| `id`  [`db/migrations/002_payments.sql:4`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L4) | `BIGSERIAL` | _PK__unique_ | no |  |
| `order_id`  [`db/migrations/002_payments.sql:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L5) | `UUID` | _FK_ → orders.id | no |  |
| `charge_id`  [`db/migrations/002_payments.sql:6`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L6) | `TEXT` | _unique_ | no |  |
| `status`  [`db/migrations/002_payments.sql:7`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L7) | `TEXT` |  | no | `CHECK (status IN ('succeeded', 'failed'))` |
| `amount_cents`  [`db/migrations/002_payments.sql:8`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L8) | `BIGINT` |  | no |  |
| `created_at`  [`db/migrations/002_payments.sql:9`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L9) | `TIMESTAMPTZ` |  | no | default `now()` |

### products

| Column | Type | Key | Nullable | Default & constraints |
|---|---|---|---|---|
| `sku`  [`db/migrations/001_init.sql:4`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L4) | `TEXT` | _PK__unique_ | no |  |
| `name`  [`db/migrations/001_init.sql:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L5) | `TEXT` |  | no |  |
| `price_cents`  [`db/migrations/001_init.sql:6`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L6) | `INTEGER` |  | no | `CHECK (price_cents >= 0)` |
| `active`  [`db/migrations/001_init.sql:7`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L7) | `BOOLEAN` |  | no | default `true` |

## Lifecycles

States come from enum and check-constraint declarations; transitions from the code that assigns them, with the guard it checks first. A transition drawn from “any state” is one the code performs without checking the current state.

### orders.status

![orders.status lifecycle](../diagrams/lifecycle-orders-status.svg)

_4 states, 3 transitions  [`db/migrations/001_init.sql:13`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L13)_ · [IR](../diagrams/lifecycle-orders-status.ir.json)

| From | To | Performed by | Guard | Code |
|---|---|---|---|---|
| `pending` | `paid` | [API Gateway](../pages/03-containers-api-gateway.md) `markOrderPaid` | — | [`api-gateway/src/db.ts:30`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L30) |
| `pending` | `cancelled` | [API Gateway](../pages/03-containers-api-gateway.md) `cancelOrder` | — | [`api-gateway/src/db.ts:35`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L35) |
| `paid` | `shipped` | [Fulfillment](../pages/03-containers-fulfillment.md) `ship_order` | — | [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14) |

