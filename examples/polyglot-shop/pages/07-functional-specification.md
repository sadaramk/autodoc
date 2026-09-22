# Functional specification

_Product_ · [Book index](../README.md)

5 functional requirements and 19 business rules as the code implements them, each traceable to the handler, rule or state change that realises it.

> [!NOTE]
> **As built** — Each requirement below is derived from an operation the code serves and what its handler does. Who performs it and why cannot be read from code: those answers come from `authored.json` and are badged _authored_; unanswered ones are marked _needs input: …_.

## API Gateway

### FR-api-gateway-get-catalog-7980 · GET /catalog

- **Actor** _needs input: actor_ · called from [Web](../pages/03-containers-web.md)
- **Purpose** GET /catalog — product list, cached in Redis for 60 seconds. (handler documentation)  `api-gateway/src/routes/catalog.ts:8-11`
- **Trigger** [GET /catalog](../pages/06-api-api-gateway.md#op-get-catalog) → `catalogRouter.get` `api-gateway/src/routes/catalog.ts:8-11`
- **Capability** [Catalog](../pages/06-api-api-gateway.md)
- **Accepts** no declared input
- **Changes state** `reads & writes` `api-gateway/src/cache.ts:9-17`
- **Returns** 200 _type not declared_

### FR-api-gateway-post-checkout-bf89 · POST /checkout

- **Actor** _needs input: actor_ · called from [Web](../pages/03-containers-web.md)
- **Purpose** POST /checkout — the critical transaction path. (handler documentation)  `api-gateway/src/routes/checkout.ts:33-48`
- **Trigger** [POST /checkout](../pages/06-api-api-gateway.md#op-post-checkout) → `checkoutRouter.post` `api-gateway/src/routes/checkout.ts:33-48`
- **Capability** [Checkout](../pages/06-api-api-gateway.md)
- **Accepts** [CheckoutRequest](../pages/06-api-api-gateway.md#model-checkoutrequest) with required items, paymentToken
- **Must satisfy** [BR-ff06d5](../pages/07-functional-specification.md#business-rules), [BR-c71e9e](../pages/07-functional-specification.md#business-rules), [BR-bb85df](../pages/07-functional-specification.md#business-rules), [BR-05050d](../pages/07-functional-specification.md#business-rules), [BR-521885](../pages/07-functional-specification.md#business-rules), [BR-55e2f4](../pages/07-functional-specification.md#business-rules), [BR-bc9e34](../pages/07-functional-specification.md#business-rules), [BR-9acd65](../pages/07-functional-specification.md#business-rules)
- **Changes state** `write orders` `api-gateway/src/db.ts:16`, `write payments` `payments/internal/ledger/ledger.go:23`, `write orders` `fulfillment/fulfillment/shipping.py:14`, `orders.status → shipped` `fulfillment/fulfillment/shipping.py:14`
- **Calls** `POST /charges` → [Payments](../pages/03-containers-payments.md) `api-gateway/src/clients/payments.ts:11`
- **Emits** `publishes order.placed` → **Kafka** `api-gateway/src/events.ts:16-21`
- **Returns** 201 [CheckoutPostResponse](../pages/06-api-api-gateway.md#model-checkoutpostresponse); fails with 400, 402

## Ledger Audit

### FR-ledger-audit-get-reports-daily-241b · GET /reports/daily

- **Actor** _needs input: actor_
- **Purpose** GET /reports/daily — today's payment totals by status. (handler documentation)  `ledger-audit/src/routes.rs:36-40`
- **Trigger** [GET /reports/daily](../pages/06-api-ledger-audit.md#op-get-reports-daily) → `daily_report` `ledger-audit/src/routes.rs:36-40`
- **Capability** [Reports](../pages/06-api-ledger-audit.md)
- **Accepts** no declared input
- **Returns** 200 [Vec&lt;DailyTotal>\[\]](../pages/06-api-ledger-audit.md#model-dailytotal)

### FR-ledger-audit-get-reports-daily-status-e7f7 · GET /reports/daily/{status}

- **Actor** _needs input: actor_
- **Purpose** GET /reports/daily/{status} — today's total for one payment status. (handler documentation)  `ledger-audit/src/routes.rs:43-46`
- **Trigger** [GET /reports/daily/{status}](../pages/06-api-ledger-audit.md#op-get-reports-daily-status) → `daily_status` `ledger-audit/src/routes.rs:43-46`
- **Capability** [Reports](../pages/06-api-ledger-audit.md)
- **Accepts** parameters status
- **Returns** 200 [DailyTotal](../pages/06-api-ledger-audit.md#model-dailytotal); fails with 404, 503

## Payments

### FR-payments-post-charges-4b48 · POST /charges

- **Actor** _needs input: actor_ · called from [API Gateway](../pages/03-containers-api-gateway.md)
- **Purpose** createCharge charges the card through Stripe and records the attempt in the ledger. (handler documentation)  `payments/internal/httpapi/handler.go:46-64`
- **Trigger** [POST /charges](../pages/06-api-payments.md#op-post-charges) → `createCharge` `payments/internal/httpapi/handler.go:46-64`
- **Capability** [Charges](../pages/06-api-payments.md)
- **Accepts** [chargeRequest](../pages/06-api-payments.md#model-chargerequest) with required orderId, amountCents, token
- **Must satisfy** [BR-cb375e](../pages/07-functional-specification.md#business-rules), [BR-4ea204](../pages/07-functional-specification.md#business-rules), [BR-e0dce6](../pages/07-functional-specification.md#business-rules)
- **Changes state** `write payments` `payments/internal/ledger/ledger.go:23`
- **Returns** 201 [chargeResponse](../pages/06-api-payments.md#model-chargeresponse); fails with 400, 402, 422

## Background processing

Requirements realised by scheduled jobs, message and event handlers and startup hooks: the system acts without a caller.

### FR-fulfillment-message-handle-order-9d47 · handle\_order · Kafka topic order.placed

- **Trigger** _message_ `Kafka topic order.placed` → [Fulfillment](../pages/03-containers-fulfillment.md) `fulfillment/fulfillment/consumer.py:19-29`
- **Changes state** `write orders` `fulfillment/fulfillment/shipping.py:14`
- **Behaviour** [sequence diagram](../pages/05-critical-flows.md#flow-fulfillment-message-handle-order)

## Business rules

Every constraint the code enforces on input, access, stored data and state changes, numbered for reference from the requirements above.

| Rule | Statement | Kind | Applies to | Code |
|---|---|---|---|---|
| **BR-ff06d5** | requires authenticated: requireCustomer | _authorization_ | `POST /checkout` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:33` |
| **BR-c71e9e** | at least 1 item | _validation_ | `CheckoutRequest.items` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:13` |
| **BR-bb85df** | at least 1 character | _validation_ | `CheckoutRequest.paymentToken` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:22` |
| **BR-05050d** | at most 32 characters | _validation_ | `CheckoutRequest.couponCode` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:23` |
| **BR-521885** | at least 1 character | _validation_ | `CheckoutRequestItem.sku` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:16` |
| **BR-55e2f4** | must be an integer | _validation_ | `CheckoutRequestItem.quantity` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:17` |
| **BR-bc9e34** | must be greater than 0 | _validation_ | `CheckoutRequestItem.quantity` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:17` |
| **BR-9acd65** | must be ≤ 99 | _validation_ | `CheckoutRequestItem.quantity` (FR-api-gateway-post-checkout-bf89) | `api-gateway/src/routes/checkout.ts:17` |
| **BR-cb375e** | must be a UUID | _validation_ | `chargeRequest.orderId` (FR-payments-post-charges-4b48) | `payments/internal/httpapi/handler.go:15` |
| **BR-4ea204** | must be > 0 | _validation_ | `chargeRequest.amountCents` (FR-payments-post-charges-4b48) | `payments/internal/httpapi/handler.go:16` |
| **BR-e0dce6** | one of: usd, eur, gbp | _validation_ | `chargeRequest.currency` (FR-payments-post-charges-4b48) | `payments/internal/httpapi/handler.go:17` |
| **BR-34ca4e** | CHECK (quantity > 0) | _data integrity_ | `order_items.quantity` | `db/migrations/001_init.sql:23` |
| **BR-7eb29c** | CHECK (total\_cents > 0) | _data integrity_ | `orders.total_cents` | `db/migrations/001_init.sql:12` |
| **BR-3db52d** | CHECK (status IN ('pending', 'paid', 'shipped', 'cancelled')) | _data integrity_ | `orders.status` | `db/migrations/001_init.sql:13` |
| **BR-b77af1** | CHECK (status IN ('succeeded', 'failed')) | _data integrity_ | `payments.status` | `db/migrations/002_payments.sql:7` |
| **BR-09680e** | CHECK (price\_cents >= 0) | _data integrity_ | `products.price_cents` | `db/migrations/001_init.sql:6` |
| **BR-fa47c7** | orders.status may become paid only from pending | _state_ | `orders.status` | `api-gateway/src/db.ts:30` |
| **BR-ff1e56** | orders.status may become cancelled only from pending | _state_ | `orders.status` | `api-gateway/src/db.ts:35` |
| **BR-ff0554** | orders.status may become shipped only from paid | _state_ | `orders.status` | `fulfillment/fulfillment/shipping.py:14` |

