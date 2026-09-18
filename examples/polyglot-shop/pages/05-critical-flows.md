# Critical flows

_Flows_ · [Book index](../README.md)

The paths that matter most, hop by hop, each pinned to the code that makes the call.

## Primary path

The critical transaction runs **Web → API Gateway → Payments → Stripe**. Step through it to see each hop highlighted on the diagram with the code that makes the call.

1. [Web](../pages/03-containers-web.md) → [API Gateway](../pages/03-containers-api-gateway.md) · HTTP — _observed in code_  [`web/src/api/client.ts:9-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L9-L19) [`web/src/api/client.ts:3`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L3) [`docker-compose.yml:7`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L7)
2. [API Gateway](../pages/03-containers-api-gateway.md) → [Payments](../pages/03-containers-payments.md) · HTTP — _observed in code_  [`api-gateway/src/clients/payments.ts:10-20`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L10-L20) [`api-gateway/src/clients/payments.ts:1`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L1) [`docker-compose.yml:24`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L24)
3. [Payments](../pages/03-containers-payments.md) → **Stripe** · charges cards — _observed in code_  [`payments/internal/charge/charge.go:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/charge/charge.go#L5)

## Request flows

What happens, in order, when each of the most involved operations is called: traced from the handler through local calls, calls to other services, reads and writes, and published events, continuing into the handlers that consume those events. Every message links to the line that sends it.

### POST /checkout

![POST /checkout](../diagrams/flow-api-gateway-post-checkout.svg)

_6 participants, 9 messages (2 asynchronous) · contract: [POST /checkout](../pages/06-api-api-gateway.md#op-post-checkout)_ · [IR](../diagrams/flow-api-gateway-post-checkout.ir.json)

| # | From → to | Message | Kind | Code |
|---|---|---|---|---|
| 1 | [Web](../pages/03-containers-web.md) → [API Gateway](../pages/03-containers-api-gateway.md) | `POST /checkout` · `CheckoutRequest` | _call_ | `submitCheckout`  [`web/src/api/client.ts:10`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L10) |
| 2 | [API Gateway](../pages/03-containers-api-gateway.md) → **PostgreSQL** | `write orders` | _write_ | `insertOrder`  [`api-gateway/src/db.ts:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L16) |
| 3 | [API Gateway](../pages/03-containers-api-gateway.md) → [Payments](../pages/03-containers-payments.md) | `POST /charges` · `chargeRequest` | _call_ | `chargeOrder`  [`api-gateway/src/clients/payments.ts:11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L11) |
| 4 | [Payments](../pages/03-containers-payments.md) → **PostgreSQL** | `write payments` | _write_ | `Record`  [`payments/internal/ledger/ledger.go:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/ledger/ledger.go#L23) |
| 5 | [Payments](../pages/03-containers-payments.md) → [API Gateway](../pages/03-containers-api-gateway.md) | `201 chargeResponse` | _reply_ | `createCharge`  [`payments/internal/httpapi/handler.go:46-64`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L46-L64) |
| 6 | [API Gateway](../pages/03-containers-api-gateway.md) → **Kafka** | `publishes order.placed` | _event_ | `publishOrderPlaced`  [`api-gateway/src/events.ts:16-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/events.ts#L16-L21) |
| 7 | [API Gateway](../pages/03-containers-api-gateway.md) → [Web](../pages/03-containers-web.md) | `201 CheckoutPostResponse` | _reply_ | `checkoutRouter.post`  [`api-gateway/src/routes/checkout.ts:33-48`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L33-L48) |
| 8 | **Kafka** → [Fulfillment](../pages/03-containers-fulfillment.md) | `delivers order.placed` | _delivered_ _async_ | `run`  [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29) |
| 9 | [Fulfillment](../pages/03-containers-fulfillment.md) → **PostgreSQL** | `write orders` | _write_ _async_ | `ship_order`  [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14) |

### GET /catalog

![GET /catalog](../diagrams/flow-api-gateway-get-catalog.svg)

_3 participants, 3 messages · contract: [GET /catalog](../pages/06-api-api-gateway.md#op-get-catalog)_ · [IR](../diagrams/flow-api-gateway-get-catalog.ir.json)

| # | From → to | Message | Kind | Code |
|---|---|---|---|---|
| 1 | [Web](../pages/03-containers-web.md) → [API Gateway](../pages/03-containers-api-gateway.md) | `GET /catalog` | _call_ | `fetchCatalog`  [`web/src/api/client.ts:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L23) |
| 2 | [API Gateway](../pages/03-containers-api-gateway.md) → **Redis** | `reads & writes` | _write_ | `cached`  [`api-gateway/src/cache.ts:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/cache.ts#L9-L17) |
| 3 | [API Gateway](../pages/03-containers-api-gateway.md) → [Web](../pages/03-containers-web.md) | `200` | _reply_ | `catalogRouter.get`  [`api-gateway/src/routes/catalog.ts:8-11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/catalog.ts#L8-L11) |

### POST /charges

![POST /charges](../diagrams/flow-payments-post-charges.svg)

_3 participants, 3 messages · contract: [POST /charges](../pages/06-api-payments.md#op-post-charges)_ · [IR](../diagrams/flow-payments-post-charges.ir.json)

| # | From → to | Message | Kind | Code |
|---|---|---|---|---|
| 1 | [API Gateway](../pages/03-containers-api-gateway.md) → [Payments](../pages/03-containers-payments.md) | `POST /charges` · `chargeRequest` | _call_ | `chargeOrder`  [`api-gateway/src/clients/payments.ts:11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L11) |
| 2 | [Payments](../pages/03-containers-payments.md) → **PostgreSQL** | `write payments` | _write_ | `Record`  [`payments/internal/ledger/ledger.go:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/ledger/ledger.go#L23) |
| 3 | [Payments](../pages/03-containers-payments.md) → [API Gateway](../pages/03-containers-api-gateway.md) | `201 chargeResponse` | _reply_ | `createCharge`  [`payments/internal/httpapi/handler.go:46-64`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L46-L64) |

### GET /reports/daily

![GET /reports/daily](../diagrams/flow-ledger-audit-get-reports-daily.svg)

_3 participants, 3 messages · contract: [GET /reports/daily](../pages/06-api-ledger-audit.md#op-get-reports-daily)_ · [IR](../diagrams/flow-ledger-audit-get-reports-daily.ir.json)

| # | From → to | Message | Kind | Code |
|---|---|---|---|---|
| 1 | **Client** → [Ledger Audit](../pages/03-containers-ledger-audit.md) | `GET /reports/daily` | _call_ | [`ledger-audit/src/routes.rs:22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L22) |
| 2 | [Ledger Audit](../pages/03-containers-ledger-audit.md) → **PostgreSQL** | `read payments` | _read_ | `daily_totals`  [`ledger-audit/src/audit.rs:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L17) |
| 3 | [Ledger Audit](../pages/03-containers-ledger-audit.md) → **Client** | `200 Vec<DailyTotal>[]` | _reply_ | `daily_report`  [`ledger-audit/src/routes.rs:36-40`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L36-L40) |

### GET /reports/daily/{status}

![GET /reports/daily/{status}](../diagrams/flow-ledger-audit-get-reports-daily-status.svg)

_3 participants, 3 messages · contract: [GET /reports/daily/{status}](../pages/06-api-ledger-audit.md#op-get-reports-daily-status)_ · [IR](../diagrams/flow-ledger-audit-get-reports-daily-status.ir.json)

| # | From → to | Message | Kind | Code |
|---|---|---|---|---|
| 1 | **Client** → [Ledger Audit](../pages/03-containers-ledger-audit.md) | `GET /reports/daily/{status}` | _call_ | [`ledger-audit/src/routes.rs:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L23) |
| 2 | [Ledger Audit](../pages/03-containers-ledger-audit.md) → **PostgreSQL** | `read payments` | _read_ | `daily_totals`  [`ledger-audit/src/audit.rs:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L17) |
| 3 | [Ledger Audit](../pages/03-containers-ledger-audit.md) → **Client** | `200 DailyTotal` | _reply_ | `daily_status`  [`ledger-audit/src/routes.rs:43-46`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L43-L46) |

## Message & event handlers

Handlers started by a message, task or in-process event, traced the same way. Steps after a hand-off to another handler are asynchronous.

### handle_order · Kafka topic order.placed

![handle\_order · Kafka topic order.placed](../diagrams/flow-fulfillment-message-handle-order.svg)

_3 participants, 2 messages_ · [IR](../diagrams/flow-fulfillment-message-handle-order.ir.json)

| # | From → to | Message | Kind | Code |
|---|---|---|---|---|
| 1 | **Kafka** → [Fulfillment](../pages/03-containers-fulfillment.md) | `delivers order.placed` | _delivered_ | `run`  [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29) |
| 2 | [Fulfillment](../pages/03-containers-fulfillment.md) → **PostgreSQL** | `write orders` | _write_ | `ship_order`  [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14) |

## Asynchronous flows

- [API Gateway](../pages/03-containers-api-gateway.md) publishes order.placed → **Kafka** → [Fulfillment](../pages/03-containers-fulfillment.md)  [`api-gateway/src/events.ts:16-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/events.ts#L16-L21) [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29)

