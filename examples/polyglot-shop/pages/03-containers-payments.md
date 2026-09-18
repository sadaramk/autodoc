# Payments

_Container_ · [Book index](../README.md)

Command payments runs the payments HTTP service.

**4** Files · **158** Lines · **4** Modules · **2** Depends on · **1** Used by

## At a glance

|   |   |
|---|---|
| Role | HTTP service |
| Technology | `Go · net/http` |
| Path | `payments/` |
| Frameworks | net/http |
| Manifest | `payments/go.mod` |
| Compose service | `payments` |
| Entry points | `func main` in package main  `payments/cmd/payments/main.go:17-30` |

## Components

![Payments — components](../diagrams/components-payments.svg)

_Components of Payments: modules and the imports between them._ · [IR](../diagrams/components-payments.ir.json)

## Modules

| Module | Path | Responsibility | Symbols | Evidence |
|---|---|---|---|---|
| **Payments** _entry_ | `payments/cmd/payments/main.go` | Command payments runs the payments HTTP service. | 1 | `payments/cmd/payments/main.go:17-30` |
| **Httpapi** | `payments/internal/httpapi/handler.go` | Package httpapi exposes the payments service over HTTP. | 6 | `payments/internal/httpapi/handler.go:28-31` |
| **Charge** | `payments/internal/charge/charge.go` | Package charge talks to Stripe to capture card payments. | 4 | `payments/internal/charge/charge.go:10-13` |
| **Ledger** | `payments/internal/ledger/ledger.go` | Package ledger records every charge attempt in Postgres. | 3 | `payments/internal/ledger/ledger.go:11-13` |

## Depends on

| Target | Interaction | Basis | Evidence |
|---|---|---|---|
| **PostgreSQL** | writes payments | _observed in code_ | `payments/internal/ledger/ledger.go:21-26` `payments/cmd/payments/main.go:13` |
| **Stripe** | charges cards | _observed in code_ | `payments/internal/charge/charge.go:5` |

## Used by

| Caller | Interaction | Basis | Evidence |
|---|---|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | HTTP | _observed in code_ | `api-gateway/src/clients/payments.ts:10-20` `api-gateway/src/clients/payments.ts:1` |

## Key symbols

| Symbol | Kind | Description | Evidence |
|---|---|---|---|
| `Result` | struct | Result is the outcome of a single charge attempt. | `payments/internal/charge/charge.go:10-13` |
| `StripeCharger` | struct | StripeCharger captures payments using the Stripe PaymentIntents API. | `payments/internal/charge/charge.go:16` |
| `Handler` | struct | Handler serves POST /charges. | `payments/internal/httpapi/handler.go:28-31` |
| `Ledger` | struct | Ledger is an append-only record of payments. | `payments/internal/ledger/ledger.go:11-13` |
| `NewStripeCharger` | function | NewStripeCharger configures the global Stripe key and returns a charger. | `payments/internal/charge/charge.go:19-22` |
| `NewHandler` | function | NewHandler builds a Handler from its collaborators. | `payments/internal/httpapi/handler.go:34-36` |
| `New` | function | New returns a Ledger backed by the given connection pool. | `payments/internal/ledger/ledger.go:16-18` |
| `Charge` | method | Charge confirms a PaymentIntent for amountCents using the client token. | `payments/internal/charge/charge.go:25-38` |
| `Routes` | method | Routes returns the service mux. | `payments/internal/httpapi/handler.go:39-43` |
| `Record` | method | Record appends a payment row for the order. | `payments/internal/ledger/ledger.go:21-26` |

