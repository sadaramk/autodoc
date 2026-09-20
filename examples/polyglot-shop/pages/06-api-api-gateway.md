# API Gateway API

_API reference_ · [Book index](../README.md)

The contract of every express operation API Gateway serves: parameters with their wire names and rules, request and response models, errors, authentication and who calls it.

**2** operations · **0** typed contracts · **1** partial · **1** opaque

> [!NOTE]
> **Contract coverage** — 0 of 2 operations declare request and response types; 1 partially; 1 not at all (the handler reads the request at runtime). 1 operational route (health, metrics, docs) listed at the end, not documented as functionality.

## Capabilities

![API Gateway — capabilities](../diagrams/capabilities-api-gateway.svg)

_2 capability groups: who calls each and what its operations touch_ · [IR](../diagrams/capabilities-api-gateway.ir.json)

## Operations

| Method | Path | Group | Request | Response | Auth | Contract |
|---|---|---|---|---|---|---|
| `GET` | [/catalog](../pages/06-api-api-gateway.md#op-get-catalog) | Catalog | — | — | _none found_ | _opaque_ |
| `POST` | [/checkout](../pages/06-api-api-gateway.md#op-post-checkout) | Checkout | [CheckoutRequest](../pages/06-api-api-gateway.md#model-checkoutrequest) | [CheckoutPostResponse](../pages/06-api-api-gateway.md#model-checkoutpostresponse) | _authenticated_ | _partial_ |

## Access

Roles, scopes and authentication the code requires, read from annotations, guards, dependencies and security configuration. "none found" means no requirement was recognised in code, not that the operation is public; expressions that are more than a plain role or scope check are shown as written.

| Operations | authenticated | no auth found |
|---|---|---|
| [GET /catalog](../pages/06-api-api-gateway.md#op-get-catalog) |  | _none found_ |
| [POST /checkout](../pages/06-api-api-gateway.md#op-post-checkout) | ✓ `api-gateway/src/routes/checkout.ts:33` |  |

## GET /catalog

GET /catalog — product list, cached in Redis for 60 seconds. Handled by `catalogRouter.get` `api-gateway/src/routes/catalog.ts:8-11` · _opaque_

- **Response 200** _type not declared_

**Called by**

- [Web](../pages/03-containers-web.md) in `fetchCatalog` `web/src/api/client.ts:23`

Behaviour: [sequence diagram](../pages/05-critical-flows.md#flow-api-gateway-get-catalog)

## POST /checkout

POST /checkout — the critical transaction path. Handled by `checkoutRouter.post` `api-gateway/src/routes/checkout.ts:33-48` · _partial_

- **Request body** [CheckoutRequest](../pages/06-api-api-gateway.md#model-checkoutrequest)
- **Response 201** [CheckoutPostResponse](../pages/06-api-api-gateway.md#model-checkoutpostresponse)
- **Requires** authenticated: requireCustomer  `api-gateway/src/routes/checkout.ts:33`

| CheckoutRequest field | Type | Required | Rules |
|---|---|---|---|
| `items` `api-gateway/src/routes/checkout.ts:13` | [CheckoutRequestItem\[\]](../pages/06-api-api-gateway.md#model-checkoutrequestitem) | yes | at least 1 item `api-gateway/src/routes/checkout.ts:13` · Cart lines to buy. |
| `paymentToken` `api-gateway/src/routes/checkout.ts:22` | `string` | yes | at least 1 character `api-gateway/src/routes/checkout.ts:22` · Card token issued by the payment form. |
| `couponCode` `api-gateway/src/routes/checkout.ts:23` | `string` | no | at most 32 characters `api-gateway/src/routes/checkout.ts:23` |

| Error | Message | Raised at |
|---|---|---|
| `400` | invalid checkout request | `api-gateway/src/routes/checkout.ts:36` |
| `402` | payment declined | `api-gateway/src/routes/checkout.ts:43` |

**Called by**

- [Web](../pages/03-containers-web.md) in `submitCheckout` `web/src/api/client.ts:10` _reads undeclared: estimatedDelivery_

Behaviour: [sequence diagram](../pages/05-critical-flows.md#flow-api-gateway-post-checkout)

## Models

### CheckoutPostResponse

Declared `api-gateway/src/routes/checkout.ts:47`

| CheckoutPostResponse field | Type | Required | Rules |
|---|---|---|---|
| `orderId` `api-gateway/src/routes/checkout.ts:47` | `unknown` | yes | — |
| `status` `api-gateway/src/routes/checkout.ts:47` | `"placed"` | yes | — |

### CheckoutRequest

Body of POST /checkout, validated before anything is persisted. Declared `api-gateway/src/routes/checkout.ts:11-24`

| CheckoutRequest field | Type | Required | Rules |
|---|---|---|---|
| `items` `api-gateway/src/routes/checkout.ts:13` | [CheckoutRequestItem\[\]](../pages/06-api-api-gateway.md#model-checkoutrequestitem) | yes | at least 1 item `api-gateway/src/routes/checkout.ts:13` · Cart lines to buy. |
| `paymentToken` `api-gateway/src/routes/checkout.ts:22` | `string` | yes | at least 1 character `api-gateway/src/routes/checkout.ts:22` · Card token issued by the payment form. |
| `couponCode` `api-gateway/src/routes/checkout.ts:23` | `string` | no | at most 32 characters `api-gateway/src/routes/checkout.ts:23` |

### CheckoutRequestItem

Declared `api-gateway/src/routes/checkout.ts:13`

| CheckoutRequestItem field | Type | Required | Rules |
|---|---|---|---|
| `sku` `api-gateway/src/routes/checkout.ts:16` | `string` | yes | at least 1 character `api-gateway/src/routes/checkout.ts:16` |
| `quantity` `api-gateway/src/routes/checkout.ts:17` | `number` | yes | must be an integer `api-gateway/src/routes/checkout.ts:17`; must be greater than 0 `api-gateway/src/routes/checkout.ts:17`; must be ≤ 99 `api-gateway/src/routes/checkout.ts:17` |

## Operational routes

| Route | Why excluded | Code |
|---|---|---|
| `GET /healthz` | health / liveness probe | `api-gateway/src/server.ts:14` |

