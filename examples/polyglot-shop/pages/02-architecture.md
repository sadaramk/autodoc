# Architecture

_Architecture_ · [Book index](../README.md)

How the 5 deployables, 3 datastores and queues, and 2 external services connect.

![Polyglot Shop — containers](../diagrams/containers.svg)

_Container view. Hover a card to trace what it depends on; click it to open its page or its source._ · [IR](../diagrams/containers.ir.json)

## Deployables

| Container | Role | Technology | Entry point | Path |
|---|---|---|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | HTTP service | `TypeScript · Express` | `api-gateway/src/server.ts:19-24` | `api-gateway/` |
| [Fulfillment](../pages/03-containers-fulfillment.md) | Background worker | `Python` | `fulfillment/fulfillment/__main__.py:22-23` | `fulfillment/` |
| [Ledger Audit](../pages/03-containers-ledger-audit.md) | HTTP service | `Rust · Axum` | `ledger-audit/src/main.rs:10-21` | `ledger-audit/` |
| [Payments](../pages/03-containers-payments.md) | HTTP service | `Go · net/http` | `payments/cmd/payments/main.go:17-30` | `payments/` |
| [Web](../pages/03-containers-web.md) | Web client | `TypeScript · React` | `web/src/main.tsx:9` | `web/` |

## Relationships

Each relationship is marked _observed in code_ when a call site, import or query was found, or _declared_ when it only appears in compose files or manifests.

| From | To | Interaction | Basis | Evidence |
|---|---|---|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | **Kafka** | publishes order.placed | _observed in code_ | `api-gateway/src/events.ts:16-21` `docker-compose.yml:22` |
| [API Gateway](../pages/03-containers-api-gateway.md) | [Payments](../pages/03-containers-payments.md) | HTTP | _observed in code_ | `api-gateway/src/clients/payments.ts:10-20` `api-gateway/src/clients/payments.ts:1` |
| [API Gateway](../pages/03-containers-api-gateway.md) | **PostgreSQL** | writes orders | _observed in code_ | `api-gateway/src/db.ts:13-20` `api-gateway/src/db.ts:29-31` |
| [API Gateway](../pages/03-containers-api-gateway.md) | **Redis** | reads & writes | _observed in code_ | `api-gateway/src/cache.ts:9-17` `api-gateway/src/cache.ts:1` |
| [Fulfillment](../pages/03-containers-fulfillment.md) | **PostgreSQL** | writes orders | _observed in code_ | `fulfillment/fulfillment/shipping.py:9-17` `fulfillment/fulfillment/shipping.py:6` |
| [Fulfillment](../pages/03-containers-fulfillment.md) | **SendGrid** | sends email | _observed in code_ | `fulfillment/fulfillment/notify.py:5` |
| **Kafka** | [Fulfillment](../pages/03-containers-fulfillment.md) | delivers order.placed | _observed in code_ | `fulfillment/fulfillment/consumer.py:19-29` `docker-compose.yml:41` |
| [Ledger Audit](../pages/03-containers-ledger-audit.md) | **PostgreSQL** | reads payments | _observed in code_ | `ledger-audit/src/audit.rs:15-22` `ledger-audit/src/audit.rs:4` |
| [Payments](../pages/03-containers-payments.md) | **PostgreSQL** | writes payments | _observed in code_ | `payments/internal/ledger/ledger.go:21-26` `payments/cmd/payments/main.go:13` |
| [Payments](../pages/03-containers-payments.md) | **Stripe** | charges cards | _observed in code_ | `payments/internal/charge/charge.go:5` |
| [Web](../pages/03-containers-web.md) | [API Gateway](../pages/03-containers-api-gateway.md) | HTTP | _observed in code_ | `web/src/api/client.ts:9-19` `web/src/api/client.ts:3` |

