# Payments API

_API reference_ · [Book index](../README.md)

The contract of every net/http operation Payments serves: parameters with their wire names and rules, request and response models, errors, authentication and who calls it.

**1** operations · **1** typed contracts · **0** partial · **0** opaque

> [!NOTE]
> **Contract coverage** — 1 of 1 operation declare request and response types; 0 partially; 0 not at all (the handler reads the request at runtime).

## Operations

| Method | Path | Request | Response | Auth | Contract |
|---|---|---|---|---|---|
| `POST` | [/charges](../pages/06-api-payments.md#op-post-charges) | [chargeRequest](../pages/06-api-payments.md#model-chargerequest) | [chargeResponse](../pages/06-api-payments.md#model-chargeresponse) | _none found_ | _typed_ |

## POST /charges

createCharge charges the card through Stripe and records the attempt in the ledger. Handled by `createCharge` [`payments/internal/httpapi/handler.go:46-64`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L46-L64) · _typed_

- **Request body** [chargeRequest](../pages/06-api-payments.md#model-chargerequest)
- **Response 201** [chargeResponse](../pages/06-api-payments.md#model-chargeresponse)

| chargeRequest field | Type | Required | Rules |
|---|---|---|---|
| `orderId` (code: OrderID) [`payments/internal/httpapi/handler.go:15`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L15) | `string` | yes | must be a UUID [`payments/internal/httpapi/handler.go:15`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L15) · OrderID identifies the order being paid. |
| `amountCents` (code: AmountCents) [`payments/internal/httpapi/handler.go:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L16) | `int64` | yes | must be > 0 [`payments/internal/httpapi/handler.go:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L16) |
| `currency` (code: Currency) [`payments/internal/httpapi/handler.go:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L17) | `string` | no | one of: usd, eur, gbp [`payments/internal/httpapi/handler.go:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L17) |
| `token` (code: Token) [`payments/internal/httpapi/handler.go:18`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L18) | `string` | yes | — |

| Error | Message | Raised at |
|---|---|---|
| `400` | bad request | [`payments/internal/httpapi/handler.go:49`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L49) |
| `402` | — | [`payments/internal/httpapi/handler.go:59`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L59) |
| `422` | orderId and a positive amountCents are required | [`payments/internal/httpapi/handler.go:53`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L53) |

**Called by**

- [API Gateway](../pages/03-containers-api-gateway.md) in `chargeOrder` [`api-gateway/src/clients/payments.ts:11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/api-gateway/src/clients/payments.ts#L11)

Behaviour: [sequence diagram](../pages/05-critical-flows.md#flow-payments-post-charges)

## Models

### chargeRequest

chargeRequest is the body of POST /charges. Declared [`payments/internal/httpapi/handler.go:13-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L13-L19)

| chargeRequest field | Type | Required | Rules |
|---|---|---|---|
| `orderId` (code: OrderID) [`payments/internal/httpapi/handler.go:15`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L15) | `string` | yes | must be a UUID [`payments/internal/httpapi/handler.go:15`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L15) · OrderID identifies the order being paid. |
| `amountCents` (code: AmountCents) [`payments/internal/httpapi/handler.go:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L16) | `int64` | yes | must be > 0 [`payments/internal/httpapi/handler.go:16`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L16) |
| `currency` (code: Currency) [`payments/internal/httpapi/handler.go:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L17) | `string` | no | one of: usd, eur, gbp [`payments/internal/httpapi/handler.go:17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L17) |
| `token` (code: Token) [`payments/internal/httpapi/handler.go:18`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L18) | `string` | yes | — |

### chargeResponse

chargeResponse reports the outcome of a charge attempt. Declared [`payments/internal/httpapi/handler.go:22-25`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L22-L25)

| chargeResponse field | Type | Required | Rules |
|---|---|---|---|
| `chargeId` (code: ChargeID) [`payments/internal/httpapi/handler.go:23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L23) | `string` | yes | — |
| `status` (code: Status) [`payments/internal/httpapi/handler.go:24`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/payments/internal/httpapi/handler.go#L24) | `string` | yes | — |

