# Functional specification

_Product_ · [Book index](../README.md)

5 functional requirements and 19 business rules as the code implements them, each traceable to the handler, rule or state change that realises it.

> [!NOTE]
> **As built** — Each requirement below is derived from an operation the code serves and what its handler does. Who performs it and why cannot be read from code: those answers come from `authored.json` and are badged _authored_; unanswered ones are marked _needs input: …_.

## API Gateway

### FR-001 · GET /catalog

- **Actor** _needs input: actor_ · called from [Web](../pages/03-containers-web.md)
- **Purpose** GET /catalog — product list, cached in Redis for 60 seconds. (handler documentation)  [`api-gateway/src/routes/catalog.ts:8-11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/catalog.ts#L8-L11)
- **Trigger** [GET /catalog](../pages/06-api-api-gateway.md#op-get-catalog) → `catalogRouter.get` [`api-gateway/src/routes/catalog.ts:8-11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/catalog.ts#L8-L11)
- **Capability** [Catalog](../pages/06-api-api-gateway.md)
- **Accepts** no declared input
- **Changes state** `reads & writes` [`api-gateway/src/cache.ts:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/cache.ts#L9-L17)
- **Returns** 200 _type not declared_

### FR-002 · POST /checkout

- **Actor** _needs input: actor_ · called from [Web](../pages/03-containers-web.md)
- **Purpose** POST /checkout — the critical transaction path. (handler documentation)  [`api-gateway/src/routes/checkout.ts:33-48`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L33-L48)
- **Trigger** [POST /checkout](../pages/06-api-api-gateway.md#op-post-checkout) → `checkoutRouter.post` [`api-gateway/src/routes/checkout.ts:33-48`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L33-L48)
- **Capability** [Checkout](../pages/06-api-api-gateway.md)
- **Accepts** [CheckoutRequest](../pages/06-api-api-gateway.md#model-checkoutrequest) with required items, paymentToken
- **Must satisfy** [BR-001](../pages/07-functional-specification.md#business-rules), [BR-002](../pages/07-functional-specification.md#business-rules), [BR-003](../pages/07-functional-specification.md#business-rules), [BR-004](../pages/07-functional-specification.md#business-rules), [BR-005](../pages/07-functional-specification.md#business-rules), [BR-006](../pages/07-functional-specification.md#business-rules), [BR-007](../pages/07-functional-specification.md#business-rules), [BR-008](../pages/07-functional-specification.md#business-rules)
- **Changes state** `write orders` [`api-gateway/src/db.ts:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L16), `write payments` [`payments/internal/ledger/ledger.go:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/ledger/ledger.go#L23), `write orders` [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14), `orders.status → shipped` [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14)
- **Calls** `POST /charges` → [Payments](../pages/03-containers-payments.md) [`api-gateway/src/clients/payments.ts:11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L11)
- **Emits** `publishes order.placed` → **Kafka** [`api-gateway/src/events.ts:16-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/events.ts#L16-L21)
- **Returns** 201 [CheckoutPostResponse](../pages/06-api-api-gateway.md#model-checkoutpostresponse); fails with 400, 402

## Ledger Audit

### FR-003 · GET /reports/daily

- **Actor** _needs input: actor_
- **Purpose** GET /reports/daily — today's payment totals by status. (handler documentation)  [`ledger-audit/src/routes.rs:36-40`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L36-L40)
- **Trigger** [GET /reports/daily](../pages/06-api-ledger-audit.md#op-get-reports-daily) → `daily_report` [`ledger-audit/src/routes.rs:36-40`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L36-L40)
- **Capability** [Reports](../pages/06-api-ledger-audit.md)
- **Accepts** no declared input
- **Returns** 200 [Vec&lt;DailyTotal>[]](../pages/06-api-ledger-audit.md#model-dailytotal)

### FR-004 · GET /reports/daily/{status}

- **Actor** _needs input: actor_
- **Purpose** GET /reports/daily/{status} — today's total for one payment status. (handler documentation)  [`ledger-audit/src/routes.rs:43-46`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L43-L46)
- **Trigger** [GET /reports/daily/{status}](../pages/06-api-ledger-audit.md#op-get-reports-daily-status) → `daily_status` [`ledger-audit/src/routes.rs:43-46`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/routes.rs#L43-L46)
- **Capability** [Reports](../pages/06-api-ledger-audit.md)
- **Accepts** parameters status
- **Returns** 200 [DailyTotal](../pages/06-api-ledger-audit.md#model-dailytotal); fails with 404, 503

## Payments

### FR-005 · POST /charges

- **Actor** _needs input: actor_ · called from [API Gateway](../pages/03-containers-api-gateway.md)
- **Purpose** createCharge charges the card through Stripe and records the attempt in the ledger. (handler documentation)  [`payments/internal/httpapi/handler.go:46-64`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L46-L64)
- **Trigger** [POST /charges](../pages/06-api-payments.md#op-post-charges) → `createCharge` [`payments/internal/httpapi/handler.go:46-64`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L46-L64)
- **Capability** [Charges](../pages/06-api-payments.md)
- **Accepts** [chargeRequest](../pages/06-api-payments.md#model-chargerequest) with required orderId, amountCents, token
- **Must satisfy** [BR-009](../pages/07-functional-specification.md#business-rules), [BR-010](../pages/07-functional-specification.md#business-rules), [BR-011](../pages/07-functional-specification.md#business-rules)
- **Changes state** `write payments` [`payments/internal/ledger/ledger.go:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/ledger/ledger.go#L23)
- **Returns** 201 [chargeResponse](../pages/06-api-payments.md#model-chargeresponse); fails with 400, 402, 422

## Background processing

Requirements realised by scheduled jobs, message and event handlers and startup hooks: the system acts without a caller.

### FR-006 · handle_order · Kafka topic order.placed

- **Trigger** _message_ `Kafka topic order.placed` → [Fulfillment](../pages/03-containers-fulfillment.md) [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29)
- **Changes state** `write orders` [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14)
- **Behaviour** [sequence diagram](../pages/05-critical-flows.md#flow-fulfillment-message-handle-order)

## Business rules

Every constraint the code enforces on input, access, stored data and state changes, numbered for reference from the requirements above.

| Rule | Statement | Kind | Applies to | Code |
|---|---|---|---|---|
| **BR-001** | requires authenticated: requireCustomer | _authorization_ | `POST /checkout` (FR-002) | [`api-gateway/src/routes/checkout.ts:33`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L33) |
| **BR-002** | at least 1 item | _validation_ | `CheckoutRequest.items` (FR-002) | [`api-gateway/src/routes/checkout.ts:13`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L13) |
| **BR-003** | at least 1 character | _validation_ | `CheckoutRequest.paymentToken` (FR-002) | [`api-gateway/src/routes/checkout.ts:22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L22) |
| **BR-004** | at most 32 characters | _validation_ | `CheckoutRequest.couponCode` (FR-002) | [`api-gateway/src/routes/checkout.ts:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L23) |
| **BR-005** | at least 1 character | _validation_ | `CheckoutRequestItem.sku` (FR-002) | [`api-gateway/src/routes/checkout.ts:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L16) |
| **BR-006** | must be an integer | _validation_ | `CheckoutRequestItem.quantity` (FR-002) | [`api-gateway/src/routes/checkout.ts:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L17) |
| **BR-007** | must be greater than 0 | _validation_ | `CheckoutRequestItem.quantity` (FR-002) | [`api-gateway/src/routes/checkout.ts:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L17) |
| **BR-008** | must be ≤ 99 | _validation_ | `CheckoutRequestItem.quantity` (FR-002) | [`api-gateway/src/routes/checkout.ts:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/routes/checkout.ts#L17) |
| **BR-009** | must be a UUID | _validation_ | `chargeRequest.orderId` (FR-005) | [`payments/internal/httpapi/handler.go:15`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L15) |
| **BR-010** | must be > 0 | _validation_ | `chargeRequest.amountCents` (FR-005) | [`payments/internal/httpapi/handler.go:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L16) |
| **BR-011** | one of: usd, eur, gbp | _validation_ | `chargeRequest.currency` (FR-005) | [`payments/internal/httpapi/handler.go:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L17) |
| **BR-012** | CHECK (quantity > 0) | _data integrity_ | `order_items.quantity` | [`db/migrations/001_init.sql:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L23) |
| **BR-013** | CHECK (total\_cents > 0) | _data integrity_ | `orders.total_cents` | [`db/migrations/001_init.sql:12`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L12) |
| **BR-014** | CHECK (status IN ('pending', 'paid', 'shipped', 'cancelled')) | _data integrity_ | `orders.status` | [`db/migrations/001_init.sql:13`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L13) |
| **BR-015** | CHECK (status IN ('succeeded', 'failed')) | _data integrity_ | `payments.status` | [`db/migrations/002_payments.sql:7`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/002_payments.sql#L7) |
| **BR-016** | CHECK (price\_cents >= 0) | _data integrity_ | `products.price_cents` | [`db/migrations/001_init.sql:6`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/db/migrations/001_init.sql#L6) |
| **BR-017** | orders.status may become paid only from pending | _state_ | `orders.status` | [`api-gateway/src/db.ts:30`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L30) |
| **BR-018** | orders.status may become cancelled only from pending | _state_ | `orders.status` | [`api-gateway/src/db.ts:35`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L35) |
| **BR-019** | orders.status may become shipped only from paid | _state_ | `orders.status` | [`fulfillment/fulfillment/shipping.py:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L14) |

