# Runtime & deployment

_Runtime_ · [Book index](../README.md)

Where the system runs: each declared environment, its workloads, ports, replicas and health checks, and how traffic enters.

1 environment declared in the repository, running 8 workloads. Everything here is read from the deployment files; nothing is inferred about hosts or clusters they don't describe.

## Docker Compose (repository root)

Defined in `docker-compose.yml`.

![Polyglot Shop — runtime (compose)](../diagrams/runtime-compose.svg)

_8 workloads · 2 entry points · dashed: starts after · connections from environment variables_ · [IR](../diagrams/runtime-compose.ir.json)

| Workload | Runs | Image / build | Ports | Operations & configuration |
|---|---|---|---|---|
| **web**  [`docker-compose.yml:2`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L2) | [Web](../pages/03-containers-web.md) | build `web/` | 5173→5173 (public) | 1 env variable |
| **api-gateway**  [`docker-compose.yml:11`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L11) | [API Gateway](../pages/03-containers-api-gateway.md) | build `api-gateway/` | 3000→3000 (public) | 4 env variables |
| **payments**  [`docker-compose.yml:26`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L26) | [Payments](../pages/03-containers-payments.md) | build `payments/` | — | 2 env variables |
| **fulfillment**  [`docker-compose.yml:34`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L34) | [Fulfillment](../pages/03-containers-fulfillment.md) | build `fulfillment/` | — | 3 env variables |
| **ledger-audit**  [`docker-compose.yml:44`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L44) | [Ledger Audit](../pages/03-containers-ledger-audit.md) | build `ledger-audit/` | — | 1 env variable |
| **db**  [`docker-compose.yml:51`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L51) | **PostgreSQL** | `postgres:16` | — | 2 env variables |
| **redis**  [`docker-compose.yml:57`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L57) | **Redis** | `redis:7` | — | — |
| **kafka**  [`docker-compose.yml:60`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L60) | **Kafka** | `redpandadata/redpanda:v24.1.1` | — | — |

### How traffic gets in

| Route / port | Workload | Kind | Declared |
|---|---|---|---|
| `5173 → 5173` | **web** | _published port_ | [`docker-compose.yml:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L5) |
| `3000 → 3000` | **api-gateway** | _published port_ | [`docker-compose.yml:14`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L14) |

