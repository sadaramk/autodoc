# API Gateway

_Container_ · [Book index](../README.md)

HTTP service built with TypeScript · Express.

**8** Files · **189** Lines · **6** Modules · **4** Depends on · **1** Used by

## At a glance

|   |   |
|---|---|
| Role | HTTP service |
| Technology | `TypeScript · Express` |
| Path | `api-gateway/` |
| Frameworks | Express |
| Manifest | `api-gateway/package.json` |
| Compose service | `api-gateway` |
| Entry points | server bootstrap `createApp().listen()`  `api-gateway/src/server.ts:19-24` |

## Components

![API Gateway — components](../diagrams/components-api-gateway.svg)

_Components of API Gateway: modules and the imports between them._ · [IR](../diagrams/components-api-gateway.ir.json)

## Modules

| Module | Path | Responsibility | Symbols | Evidence |
|---|---|---|---|---|
| **Server** _entry_ | `api-gateway/src/server.ts` | Builds the HTTP application with all public routes mounted. | 2 | `api-gateway/src/server.ts:19-24` |
| **DB** | `api-gateway/src/db.ts` | Inserts a new pending order and returns it. | 5 | `api-gateway/src/db.ts:6-10` |
| **Clients** | `api-gateway/src/clients/payments.ts` | Synchronously charges an order via the Go payments service. | 2 | `api-gateway/src/clients/payments.ts:4-7` |
| **Events** | `api-gateway/src/events.ts` | Connects the shared Kafka producer; called once at startup. | 2 | `api-gateway/src/events.ts:11-13` |
| **Routes** | `api-gateway/src/routes/` | Rejects requests without a customer session token. | 2 | `api-gateway/src/routes/auth.ts:4-10` |
| **Cache** | `api-gateway/src/cache.ts` | Read-through cache: returns the cached JSON value for `key`, | 1 | `api-gateway/src/cache.ts:9-17` |

## Depends on

| Target | Interaction | Basis | Evidence |
|---|---|---|---|
| **Kafka** | publishes order.placed | _observed in code_ | `api-gateway/src/events.ts:16-21` `docker-compose.yml:22` |
| [Payments](../pages/03-containers-payments.md) | HTTP | _observed in code_ | `api-gateway/src/clients/payments.ts:10-20` `api-gateway/src/clients/payments.ts:1` |
| **PostgreSQL** | writes orders | _observed in code_ | `api-gateway/src/db.ts:13-20` `api-gateway/src/db.ts:29-31` |
| **Redis** | reads & writes | _observed in code_ | `api-gateway/src/cache.ts:9-17` `api-gateway/src/cache.ts:1` |

## Used by

| Caller | Interaction | Basis | Evidence |
|---|---|---|---|
| [Web](../pages/03-containers-web.md) | HTTP | _observed in code_ | `web/src/api/client.ts:9-19` `web/src/api/client.ts:3` |

## Key symbols

| Symbol | Kind | Description | Evidence |
|---|---|---|---|
| `ChargeResult` | interface | Result of a charge attempt reported by the payments service. | `api-gateway/src/clients/payments.ts:4-7` |
| `Order` | interface | An order row as stored in Postgres. | `api-gateway/src/db.ts:6-10` |
| `cached` | function | Read-through cache: returns the cached JSON value for `key`, | `api-gateway/src/cache.ts:9-17` |
| `chargeOrder` | function | Synchronously charges an order via the Go payments service. | `api-gateway/src/clients/payments.ts:10-20` |
| `insertOrder` | function | Inserts a new pending order and returns it. | `api-gateway/src/db.ts:13-20` |
| `listProducts` | function | Lists all active products for the catalog. | `api-gateway/src/db.ts:23-26` |
| `markOrderPaid` | function | Marks a pending order paid once its charge succeeded. | `api-gateway/src/db.ts:29-31` |
| `cancelOrder` | function | Cancels an order that has not been paid yet. | `api-gateway/src/db.ts:34-36` |
| `connectProducer` | function | Connects the shared Kafka producer; called once at startup. | `api-gateway/src/events.ts:11-13` |
| `publishOrderPlaced` | function | Publishes `order.placed` so fulfillment can ship the order asynchronously. | `api-gateway/src/events.ts:16-21` |

