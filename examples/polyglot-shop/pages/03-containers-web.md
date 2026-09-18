# Web

_Container_ · [Book index](../README.md)

Web client built with TypeScript · React.

**4** Files · **88** Lines · **3** Modules · **1** Depends on · **0** Used by

## At a glance

|   |   |
|---|---|
| Role | Web client |
| Technology | `TypeScript · React` |
| Path | `web/` |
| Frameworks | React, Vite |
| Manifest | `web/package.json` |
| Compose service | `web` |
| Entry points | client mount point  [`web/src/main.tsx:9`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/main.tsx#L9) |

## Components

![Web — components](../diagrams/components-web.svg)

_Components of Web: modules and the imports between them._ · [IR](../diagrams/components-web.ir.json)

## Modules

| Module | Path | Responsibility | Symbols | Evidence |
|---|---|---|---|---|
| **Main** _entry_ | `web/src/main.tsx` |  | 1 | [`web/src/main.tsx:9`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/main.tsx#L9) |
| **API** | `web/src/api/` | Submits the cart to the api-gateway checkout endpoint. | 4 | [`web/src/api/types.ts:2-7`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/types.ts#L2-L7) |
| **Pages** | `web/src/pages/Checkout.tsx` | Checkout page: lists the catalog and places an order for the whole cart. | 2 | [`web/src/pages/Checkout.tsx:6-32`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/pages/Checkout.tsx#L6-L32) |

## Depends on

| Target | Interaction | Basis | Evidence |
|---|---|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | HTTP | _observed in code_ | [`web/src/api/client.ts:9-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L9-L19) [`web/src/api/client.ts:3`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L3) |

## Key symbols

| Symbol | Kind | Description | Evidence |
|---|---|---|---|
| `CartItem` | interface | A single line in the shopping cart. | [`web/src/api/types.ts:2-7`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/types.ts#L2-L7) |
| `CheckoutResult` | interface | Response returned by POST /checkout. | [`web/src/api/types.ts:10-15`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/types.ts#L10-L15) |
| `submitCheckout` | function | Submits the cart to the api-gateway checkout endpoint. | [`web/src/api/client.ts:9-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L9-L19) |
| `fetchCatalog` | function | Loads the product catalog (served from the gateway's Redis cache). | [`web/src/api/client.ts:22-25`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/api/client.ts#L22-L25) |
| `Checkout` | function | Checkout page: lists the catalog and places an order for the whole cart. | [`web/src/pages/Checkout.tsx:6-32`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/web/src/pages/Checkout.tsx#L6-L32) |

