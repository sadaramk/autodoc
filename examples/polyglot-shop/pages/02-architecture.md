# Architecture

_Architecture_ · [Book index](../README.md)

How the 5 deployables, 3 datastores and queues, and 2 external services connect.

![Polyglot Shop — containers](../diagrams/containers.svg)

_Container view. Hover a card to trace what it depends on; click it to open its page or its source._ · [IR](../diagrams/containers.ir.json)

## Deployables

| Container | Role | Technology | Entry point | Path |
|---|---|---|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | HTTP service | `TypeScript · Express` | [`api-gateway/src/server.ts:19-24`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/server.ts#L19-L24) | `api-gateway/` |
| [Fulfillment](../pages/03-containers-fulfillment.md) | Background worker | `Python` | [`fulfillment/fulfillment/__main__.py:22-23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/__main__.py#L22-L23) | `fulfillment/` |
| [Ledger Audit](../pages/03-containers-ledger-audit.md) | HTTP service | `Rust · Axum` | [`ledger-audit/src/main.rs:10-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/main.rs#L10-L21) | `ledger-audit/` |
| [Payments](../pages/03-containers-payments.md) | HTTP service | `Go · net/http` | [`payments/cmd/payments/main.go:17-30`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/cmd/payments/main.go#L17-L30) | `payments/` |
| [Web](../pages/03-containers-web.md) | Web client | `TypeScript · React` | [`web/src/main.tsx:9`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/main.tsx#L9) | `web/` |

## Relationships

Each relationship is marked _observed in code_ when a call site, import or query was found, or _declared_ when it only appears in compose files or manifests.

| From | To | Interaction | Basis | Evidence |
|---|---|---|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | **Kafka** | publishes order.placed | _observed in code_ | [`api-gateway/src/events.ts:16-21`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/events.ts#L16-L21) [`docker-compose.yml:22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L22) |
| [API Gateway](../pages/03-containers-api-gateway.md) | [Payments](../pages/03-containers-payments.md) | HTTP | _observed in code_ | [`api-gateway/src/clients/payments.ts:10-20`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L10-L20) [`api-gateway/src/clients/payments.ts:1`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L1) |
| [API Gateway](../pages/03-containers-api-gateway.md) | **PostgreSQL** | writes orders | _observed in code_ | [`api-gateway/src/db.ts:13-20`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L13-L20) [`api-gateway/src/db.ts:29-31`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/db.ts#L29-L31) |
| [API Gateway](../pages/03-containers-api-gateway.md) | **Redis** | reads & writes | _observed in code_ | [`api-gateway/src/cache.ts:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/cache.ts#L9-L17) [`api-gateway/src/cache.ts:1`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/cache.ts#L1) |
| [Fulfillment](../pages/03-containers-fulfillment.md) | **PostgreSQL** | writes orders | _observed in code_ | [`fulfillment/fulfillment/shipping.py:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L9-L17) [`fulfillment/fulfillment/shipping.py:6`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L6) |
| [Fulfillment](../pages/03-containers-fulfillment.md) | **SendGrid** | sends email | _observed in code_ | [`fulfillment/fulfillment/notify.py:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/notify.py#L5) |
| **Kafka** | [Fulfillment](../pages/03-containers-fulfillment.md) | delivers order.placed | _observed in code_ | [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29) [`docker-compose.yml:41`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L41) |
| [Ledger Audit](../pages/03-containers-ledger-audit.md) | **PostgreSQL** | reads payments | _observed in code_ | [`ledger-audit/src/audit.rs:15-22`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L15-L22) [`ledger-audit/src/audit.rs:4`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/ledger-audit/src/audit.rs#L4) |
| [Payments](../pages/03-containers-payments.md) | **PostgreSQL** | writes payments | _observed in code_ | [`payments/internal/ledger/ledger.go:21-26`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/ledger/ledger.go#L21-L26) [`payments/cmd/payments/main.go:13`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/cmd/payments/main.go#L13) |
| [Payments](../pages/03-containers-payments.md) | **Stripe** | charges cards | _observed in code_ | [`payments/internal/charge/charge.go:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/charge/charge.go#L5) |
| [Web](../pages/03-containers-web.md) | [API Gateway](../pages/03-containers-api-gateway.md) | HTTP | _observed in code_ | [`web/src/api/client.ts:9-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L9-L19) [`web/src/api/client.ts:3`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L3) |

