# Evidence & unknowns

_Evidence_ · [Book index](../README.md)

138 of 138 citations verified. What was observed, what was only declared, and what static analysis can't see.

**138** Citations · **138** Verified · **0** Stale · **0** Broken

## Observed versus declared

11 of 11 relationships were _observed in code_; 0 are only _declared_ in compose files or manifests and may not be exercised at runtime.

## What remains unknown

- Figure components-fulfillment: dropped `fulfillment.fulfillment`: no detected relationships.
- This book is read from the source, not from the running system. Static analysis doesn't see runtime service discovery, reflection, dynamically built URLs, configuration injected at deploy time, generated code, or infrastructure provisioned outside this repository. Treat it as a well-evidenced starting point to review, not as the authority on the architecture.

## Citation index

| Location | Symbol | State | Detail |
|---|---|---|---|
| `README.md:3` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/cache.ts:1` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/cache.ts:9-17` | `cached` | _verified_ | verified against the pinned commit |
| `api-gateway/src/clients/payments.ts:1` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/clients/payments.ts:4-7` | `ChargeResult` | _verified_ | verified against the pinned commit |
| `api-gateway/src/clients/payments.ts:10-20` | `chargeOrder` | _verified_ | verified against the pinned commit |
| `api-gateway/src/clients/payments.ts:11` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:1` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:6-10` | `Order` | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:13-20` | `insertOrder` | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:16` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:23-26` | `listProducts` | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:24` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:29-31` | `markOrderPaid` | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:30` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:34-36` | `cancelOrder` | _verified_ | verified against the pinned commit |
| `api-gateway/src/db.ts:35` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/events.ts:1` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/events.ts:11-13` | `connectProducer` | _verified_ | verified against the pinned commit |
| `api-gateway/src/events.ts:16-21` | `publishOrderPlaced` | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/auth.ts:4-10` | `requireCustomer` | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/catalog.ts:8-11` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:11-24` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:13` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:16` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:17` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:22` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:23` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:33` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:33-48` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:36` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:43` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/routes/checkout.ts:47` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/server.ts:14` |  | _verified_ | verified against the pinned commit |
| `api-gateway/src/server.ts:19-24` | `main` | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:3` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:4` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:5` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:6` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:7` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:10` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:11` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:12` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:13` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:14` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:15` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:16` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:17` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:20` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:21` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:22` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:23` |  | _verified_ | verified against the pinned commit |
| `db/migrations/001_init.sql:24` |  | _verified_ | verified against the pinned commit |
| `db/migrations/002_payments.sql:3` |  | _verified_ | verified against the pinned commit |
| `db/migrations/002_payments.sql:4` |  | _verified_ | verified against the pinned commit |
| `db/migrations/002_payments.sql:5` |  | _verified_ | verified against the pinned commit |
| `db/migrations/002_payments.sql:6` |  | _verified_ | verified against the pinned commit |
| `db/migrations/002_payments.sql:7` |  | _verified_ | verified against the pinned commit |
| `db/migrations/002_payments.sql:8` |  | _verified_ | verified against the pinned commit |
| `db/migrations/002_payments.sql:9` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:2` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:5` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:7` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:11` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:14` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:22` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:24` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:26` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:34` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:41` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:44` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:51` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:57` |  | _verified_ | verified against the pinned commit |
| `docker-compose.yml:60` |  | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/__init__.py:1-3` |  | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/__main__.py:10-13` | `handle_order` | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/__main__.py:16-19` | `main` | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/__main__.py:22-23` | `__main__` | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/consumer.py:11-29` | `OrderConsumer` | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/consumer.py:19-29` | `run` | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/notify.py:5` |  | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/notify.py:9-19` | `send_shipped_email` | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/shipping.py:6` |  | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/shipping.py:9-17` | `ship_order` | _verified_ | verified against the pinned commit |
| `fulfillment/fulfillment/shipping.py:14` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/audit.rs:4` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/audit.rs:8-12` | `DailyTotal` | _verified_ | verified against the pinned commit |
| `ledger-audit/src/audit.rs:9` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/audit.rs:10` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/audit.rs:11` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/audit.rs:15-22` | `daily_totals` | _verified_ | verified against the pinned commit |
| `ledger-audit/src/audit.rs:17` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/main.rs:10-21` | `main` | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:15-17` | `router` | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:22` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:23` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:29-33` | `DailyQuery` | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:32` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:36-40` | `daily_report` | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:43` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:43-46` | `daily_status` | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:44` |  | _verified_ | verified against the pinned commit |
| `ledger-audit/src/routes.rs:45` |  | _verified_ | verified against the pinned commit |
| `payments/cmd/payments/main.go:13` |  | _verified_ | verified against the pinned commit |
| `payments/cmd/payments/main.go:17-30` | `main` | _verified_ | verified against the pinned commit |
| `payments/internal/charge/charge.go:5` |  | _verified_ | verified against the pinned commit |
| `payments/internal/charge/charge.go:10-13` | `Result` | _verified_ | verified against the pinned commit |
| `payments/internal/charge/charge.go:16` | `StripeCharger` | _verified_ | verified against the pinned commit |
| `payments/internal/charge/charge.go:19-22` | `NewStripeCharger` | _verified_ | verified against the pinned commit |
| `payments/internal/charge/charge.go:25-38` | `Charge` | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:13-19` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:15` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:16` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:17` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:18` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:22-25` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:23` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:24` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:28-31` | `Handler` | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:34-36` | `NewHandler` | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:39-43` | `Routes` | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:46-64` | `createCharge` | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:49` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:53` |  | _verified_ | verified against the pinned commit |
| `payments/internal/httpapi/handler.go:59` |  | _verified_ | verified against the pinned commit |
| `payments/internal/ledger/ledger.go:11-13` | `Ledger` | _verified_ | verified against the pinned commit |
| `payments/internal/ledger/ledger.go:16-18` | `New` | _verified_ | verified against the pinned commit |
| `payments/internal/ledger/ledger.go:21-26` | `Record` | _verified_ | verified against the pinned commit |
| `payments/internal/ledger/ledger.go:23` |  | _verified_ | verified against the pinned commit |
| `web/src/api/client.ts:3` |  | _verified_ | verified against the pinned commit |
| `web/src/api/client.ts:9-19` | `submitCheckout` | _verified_ | verified against the pinned commit |
| `web/src/api/client.ts:10` |  | _verified_ | verified against the pinned commit |
| `web/src/api/client.ts:22-25` | `fetchCatalog` | _verified_ | verified against the pinned commit |
| `web/src/api/client.ts:23` |  | _verified_ | verified against the pinned commit |
| `web/src/api/types.ts:2-7` | `CartItem` | _verified_ | verified against the pinned commit |
| `web/src/api/types.ts:10-15` | `CheckoutResult` | _verified_ | verified against the pinned commit |
| `web/src/main.tsx:9` | `createRoot` | _verified_ | verified against the pinned commit |
| `web/src/pages/Checkout.tsx:6-32` | `Checkout` | _verified_ | verified against the pinned commit |

