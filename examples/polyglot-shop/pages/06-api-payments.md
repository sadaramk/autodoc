# Payments API

_API reference_ · [Book index](../README.md)

The contract of every net/http operation Payments serves: parameters with their wire names and rules, request and response models, errors, authentication and who calls it.

**1** operations · **1** typed contracts · **0** partial · **0** opaque

> [!NOTE]
> **Contract coverage** — 1 of 1 operation declare request and response types; 0 partially; 0 not at all (the handler reads the request at runtime).

> [!WARNING]
> **Authentication not detected** — No authentication requirement was recognised on any of these operations. What a gateway, a service mesh, or a shared server package applies before the request arrives is not visible in this service's code, so this is not evidence that the operations are public — check how the service is deployed.

## Operations

| Method | Path | Request | Response | Auth | Contract |
|---|---|---|---|---|---|
| `POST` | [/charges](../pages/06-api-payments.md#op-post-charges) | [chargeRequest](../pages/06-api-payments.md#model-chargerequest) | [chargeResponse](../pages/06-api-payments.md#model-chargeresponse) | _none found_ | _typed_ |

## POST /charges

createCharge charges the card through Stripe and records the attempt in the ledger. Handled by `createCharge` `payments/internal/httpapi/handler.go:46-64` · _typed_

- **Request body** [chargeRequest](../pages/06-api-payments.md#model-chargerequest)
- **Response 201** [chargeResponse](../pages/06-api-payments.md#model-chargeresponse)

| chargeRequest field | Type | Required | Rules |
|---|---|---|---|
| `orderId` (code: OrderID) `payments/internal/httpapi/handler.go:15` | `string` | yes | must be a UUID `payments/internal/httpapi/handler.go:15` · OrderID identifies the order being paid. |
| `amountCents` (code: AmountCents) `payments/internal/httpapi/handler.go:16` | `int64` | yes | must be > 0 `payments/internal/httpapi/handler.go:16` |
| `currency` (code: Currency) `payments/internal/httpapi/handler.go:17` | `string` | no | one of: usd, eur, gbp `payments/internal/httpapi/handler.go:17` |
| `token` (code: Token) `payments/internal/httpapi/handler.go:18` | `string` | yes | — |

| Error | Message | Raised at |
|---|---|---|
| `400` | bad request | `payments/internal/httpapi/handler.go:49` |
| `402` | — | `payments/internal/httpapi/handler.go:59` |
| `422` | orderId and a positive amountCents are required | `payments/internal/httpapi/handler.go:53` |

**Called by**

- [API Gateway](../pages/03-containers-api-gateway.md) in `chargeOrder` `api-gateway/src/clients/payments.ts:11`

Behaviour: [sequence diagram](../pages/05-critical-flows.md#flow-payments-post-charges)

## Models

### chargeRequest

chargeRequest is the body of POST /charges. Declared `payments/internal/httpapi/handler.go:13-19`

| chargeRequest field | Type | Required | Rules |
|---|---|---|---|
| `orderId` (code: OrderID) `payments/internal/httpapi/handler.go:15` | `string` | yes | must be a UUID `payments/internal/httpapi/handler.go:15` · OrderID identifies the order being paid. |
| `amountCents` (code: AmountCents) `payments/internal/httpapi/handler.go:16` | `int64` | yes | must be > 0 `payments/internal/httpapi/handler.go:16` |
| `currency` (code: Currency) `payments/internal/httpapi/handler.go:17` | `string` | no | one of: usd, eur, gbp `payments/internal/httpapi/handler.go:17` |
| `token` (code: Token) `payments/internal/httpapi/handler.go:18` | `string` | yes | — |

### chargeResponse

chargeResponse reports the outcome of a charge attempt. Declared `payments/internal/httpapi/handler.go:22-25`

| chargeResponse field | Type | Required | Rules |
|---|---|---|---|
| `chargeId` (code: ChargeID) `payments/internal/httpapi/handler.go:23` | `string` | yes | — |
| `status` (code: Status) `payments/internal/httpapi/handler.go:24` | `string` | yes | — |

