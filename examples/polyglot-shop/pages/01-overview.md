# Polyglot Shop

_Overview_ · [Book index](../README.md)

A small checkout platform used as the autodoc-engine demo repository.

**5** [Deployables](../pages/02-architecture.md) · **3** [Datastores & queues](../pages/04-data-and-integrations.md) · **2** [External services](../pages/04-data-and-integrations.md) · **24 files** Source

## What it is

A small checkout platform used as the autodoc-engine demo repository.  [`README.md:3`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/README.md#L3)

Polyglot Shop builds [API Gateway](../pages/03-containers-api-gateway.md), [Fulfillment](../pages/03-containers-fulfillment.md), [Ledger Audit](../pages/03-containers-ledger-audit.md), [Payments](../pages/03-containers-payments.md) and [Web](../pages/03-containers-web.md) — deployables written mainly in TypeScript, Python, Go. State lives in **PostgreSQL**, **Redis** and **Kafka**. It calls **Stripe** and **SendGrid** outside its trust boundary.

## System context

![Polyglot Shop — system context](../diagrams/system-context.svg)

_System context: who uses the system and what it depends on outside the repository._ · [IR](../diagrams/system-context.ir.json)

## Read next

- [Architecture](../pages/02-architecture.md) — Every deployable, datastore and external service, and how they connect.
- [API Gateway](../pages/03-containers-api-gateway.md) — HTTP service · TypeScript · Express
- [Fulfillment](../pages/03-containers-fulfillment.md) — Ships placed orders and notifies customers
- [Ledger Audit](../pages/03-containers-ledger-audit.md) — Read-only reconciliation reports over the payments ledger
- [Payments](../pages/03-containers-payments.md) — Command payments runs the payments HTTP service.
- [Web](../pages/03-containers-web.md) — Web client · TypeScript · React
- [Data & integrations](../pages/04-data-and-integrations.md) — Who reads and writes which store, which events flow where, and which vendors are called.
- [Critical flows](../pages/05-critical-flows.md) — The main path through the system, hop by hop, each step pinned to code.
- [Evidence & unknowns](../pages/09-evidence-and-unknowns.md) — What was verified, what was only declared, and what static analysis cannot see.

