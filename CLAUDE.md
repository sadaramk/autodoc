# autodoc-engine

Rust workspace (`crates/*`), a TypeScript zod mirror (`packages/ir-spec-ts`), Playwright journeys (`tests/e2e`).

## Running tests (Docker)

| Layer | Command |
|-------|---------|
| Rust unit + integration + CLI/MCP journeys | `docker compose run --rm test` |
| TypeScript contract tests | `docker compose run --rm test-ts` |
| Browser journeys (built release binary) | `make e2e` |
| fmt + clippy | `docker compose run --rm lint` |

## Critical journeys

Each must pass before merge.

1. **Generate the architecture book** — `autodoc generate` → book with verified citations and permalinks; unchanged
   commit rewrites nothing; hand-edited IR kept; `autodoc check` fails on drift.
   `crates/cli/tests/journeys.rs::journey_init_generate_book_edit_and_check_in_ci`, `crates/book/tests/book.rs`
2. **Agent self-heal** — a rejected IR's patches make it compile.
   `journeys.rs::journey_agent_self_heals_a_rejected_ir`
3. **MCP agent loop** — initialize → scan → compile → density rejection → verify evidence over stdio.
   `journeys.rs::journey_mcp_agent_scans_compiles_and_verifies`
4. **Evidence drift** — editing pinned lines turns evidence stale.
   `journeys.rs::journey_evidence_drift_is_detected_after_code_changes`
5. **Behaviour documentation** — the book documents API contracts, request-flow sequences, the data model and
   lifecycles, a functional spec and a BRD scaffold; `authored.json` is created once, shown as authored, never
   overwritten; background (scheduled / message / event) flows continue across services; large APIs split into
   capability pages with an access matrix; large data models split by domain; modular monoliths get module pages;
   declared Compose/Kubernetes environments get a runtime page. `crates/book/tests/book.rs`, `api_structure.rs`,
   `crates/analyzer/tests/flows.rs`, `capabilities.rs`, `domains.rs`, `real_world_shapes.rs::runtime_topology_*`
6. **Interactive pages** — single-diagram page (`tests/e2e/specs/diagram.spec.ts`) and the book reader: navigation,
   figure drill-down, citation popovers, flow walkthrough, search, theme, mobile (`tests/e2e/specs/book.spec.ts`).

## Change → what to run

- `ir-spec` field changes: regenerate `schema/diagram-ir.schema.json` (`autodoc schema`), update
  `packages/ir-spec-ts/src/index.ts`, run `test` and `test-ts` (contract fixtures in `tests/contract`).
- `renderer` layout: `crates/renderer/tests/geometry.rs` (fixtures, 300 random graphs, random sequence / ER /
  lifecycle diagrams) and `make e2e`.
- `renderer/src/assets/*.js|css`: `make e2e`.
- `book` pages or `analyzer` output shape: `crates/book/tests/book.rs`, journey 1, `make e2e`, then `make demo`.
- `analyzer` behaviour model (`api`, `data`, `trace`, `views`): `crates/analyzer/tests/api_contracts.rs`,
  `data_model.rs`, journey 5, then `make demo`.
- JVM support (`extract.rs` Java, `manifest.rs` Maven/Gradle, `jvm.rs`, `catalog.rs`): `real_world_shapes.rs::spring_*`,
  `api_contracts.rs`, `data_model.rs`, then spot-check piggymetrics / thingsboard / spring-modulith examples.
- `analyzer` heuristics: `crates/analyzer/tests/fixtures.rs`, journeys 1–4, then `make demo` to refresh examples.
