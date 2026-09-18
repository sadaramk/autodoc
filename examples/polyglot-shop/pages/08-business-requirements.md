# Business requirements

_Product_ · [Book index](../README.md)

A business requirements document scaffolded from the implementation: what is built is measured, why and for whom is authored, and every unanswered question is listed.

> [!WARNING]
> **Business intent is authored, not generated** — Scope, capabilities, rules and integrations below are measured from code. Problem, goals, stakeholders and success metrics cannot be: they are read from `authored.json` and shown as open questions until someone answers them.

## 1. Problem & purpose

_needs input: the problem this system solves, for whom_

_from README_ A small checkout platform used as the autodoc demo repository. `README.md:3`

## 2. Goals & non-goals

**Goals**

- _needs input: measurable goals_

**Non-goals**

- _needs input: what is explicitly out of scope_

## 3. Stakeholders & users

_needs input: stakeholders, their roles and what they need_

_from code_ user-facing clients: [Web](../pages/03-containers-web.md). 

## 4. Scope: capabilities as built

| Service | Capabilities |
|---|---|
| [API Gateway](../pages/03-containers-api-gateway.md) | [FR-001 GET /catalog](../pages/07-functional-specification.md#fr-001), [FR-002 POST /checkout](../pages/07-functional-specification.md#fr-002) |
| [Ledger Audit](../pages/03-containers-ledger-audit.md) | [FR-003 GET /reports/daily](../pages/07-functional-specification.md#fr-003), [FR-004 GET /reports/daily/{status}](../pages/07-functional-specification.md#fr-004) |
| [Payments](../pages/03-containers-payments.md) | [FR-005 POST /charges](../pages/07-functional-specification.md#fr-005) |

## 5. Business rules

19 rules enforced in code; see [the rules catalog](../pages/07-functional-specification.md#business-rules).

## 6. Data & integrations

4 persisted entities; external services: Stripe, SendGrid. Details on [Data & integrations](../pages/04-data-and-integrations.md).

## 7. Success metrics

_needs input: how success is measured, with targets_

## 8. Assumptions & constraints

**Assumptions**

- _needs input: assumptions_

**Constraints**

- _needs input: business, legal or operational constraints_

## 9. Open questions

- 5 of 5 requirements have no authored actor or purpose.
- [FR-001](../pages/07-functional-specification.md#fr-001) GET /catalog declares no request or response type: what does it accept and return?
- [Web](../pages/03-containers-web.md) reads estimatedDelivery from POST /checkout, which the operation does not declare `web/src/api/client.ts:10` — CheckoutPostResponse declares orderId, status `api-gateway/src/routes/checkout.ts:47`

