# polyglot-shop

A small checkout platform used as the autodoc-engine demo repository.

| Unit | Language | Role |
|------|----------|------|
| `web/` | TypeScript (React + Vite) | Storefront client |
| `api-gateway/` | TypeScript (Express) | Checkout API, caches catalog in Redis, writes orders, publishes `order.placed` |
| `payments/` | Go (net/http) | Charges cards through Stripe and records payments |
| `fulfillment/` | Python | Consumes `order.placed`, ships orders, emails customers via SendGrid |
| `ledger-audit/` | Rust (axum) | Read-only reconciliation reports over the payments table |

Infrastructure: Postgres (`db`), Redis (`redis`), Redpanda/Kafka (`kafka`).

Checkout path: web → api-gateway → payments → Stripe, then api-gateway → Kafka → fulfillment.
