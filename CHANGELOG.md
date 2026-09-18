# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0 the
`DiagramIR` schema and the CLI surface may still change between minor versions;
`version` in every IR file says which schema it was written against.

## [Unreleased]

### Added

- Command-bus dispatch is resolved by the message type: `mediatr.Send[*CreateOrder,
  *Res](ctx, command)` names the command rather than the handler, so a CQRS
  service produced no request flow at all. The handler is taken to be the one
  method that accepts the message, which holds for any bus that dispatches by
  type rather than for one library's API.
- Go request flows are traced through struct field chains, ports and store
  clients. `h.app.Queries.AllTrainings.Handle` names no function — `Handle` is
  declared on every handler in a CQRS service — so the analyzer now records
  method receivers, struct field types and interface method sets, and walks the
  receiver expression to the type that owns the body. A port resolves to its one
  adapter, and Go's exported/unexported handler pair to the struct that carries
  the implementation.
- A field typed from an infrastructure package is recognised as a handle on it,
  so a store call is cited at the line that makes it rather than at the `go.mod`
  line that declares the dependency, and is attributed to the service that calls
  it. Where the data model already reads the query it remains the source, since
  it names the table and the operation exactly.

### Security

- A file path recorded in an existing `manifest.json` can no longer reach outside
  the book directory. `generate` prunes the files a previous run produced, and
  those keys are untrusted — a book may be generated for a repository the user
  does not control — so `../../id_rsa` escaped the output directory, and an
  absolute path replaced it entirely. Reading a hand-edited diagram had the same
  flaw. Paths are now restricted to plain relative components and confirmed to
  resolve inside the book.

### Fixed

- Source files above the size limit are named on the evidence page instead of
  being skipped silently, which had let a repository with large generated or
  vendored sources produce documentation that was confidently incomplete.
- Generated mocks (`mocks/`, as mockery and gomock write them) are no longer
  documented as architecture; a flow had been citing a test double as the code
  that reads products.
- Edge ids are kept within the 80 characters the IR accepts. Deeply nested
  package layouts made the joined `source--target` pair overrun, and every edge
  in the figure then failed validation — 19 errors on one component diagram of a
  Go service laid out in feature folders.
- A module is named from more than its last path segment. Feature-folder layouts
  put the layer last, so six modules came out called "Dtos" and two called "App"
  on a single component diagram; a layer segment is now qualified by the folder
  above it and version segments are skipped.

## [0.1.0] - 2026-09-18

First public release.

### Added

- **Architecture books.** `autodoc generate` writes a self-contained book:
  interactive HTML, a Markdown mirror for GitHub, `llms.txt` for agents, and the
  typed `DiagramIR` behind every figure, which can be edited by hand and kept.
- **Verified evidence.** Every claim links to a file and line, pinned to a
  commit and verified against it. `autodoc check` fails when the code has moved
  under the documentation, which makes it usable in CI.
- **Languages.** Rust, TypeScript, Go, Python, Java and Kotlin.
- **Frameworks.** Spring MVC and WebFlux, JAX-RS, Quarkus, Micronaut, Dropwizard,
  Jersey, Helidon, Ktor, Express, FastAPI, chi and Echo; JPA/Hibernate, Spring
  Data, Exposed, Panache, Prisma, SQLAlchemy, Liquibase and Flyway; Kafka,
  RabbitMQ and in-process application events.
- **Diagrams chosen for the question they answer.** System context, container,
  component, data flow, sequence, entity-relationship and lifecycle, each held to
  a density budget with orthogonal routing and at most two focal points.
- **Behaviour model.** API reference with parameters, validation rules, request
  and response models and callers; a data model with keys, cardinality and
  lifecycles; request, scheduled and event-driven flows traced hop by hop.
- **Deployment topology.** Docker Compose with overlays, Kubernetes workloads,
  Services and Ingress, and value-only Helm templates.
- **Agent interface.** An MCP stdio server exposing scan, compile and verify, so
  an agent can draft an IR, have it rejected with JSON Patch diagnostics, and fix
  it without emitting SVG itself.
- **Business documentation.** A functional specification and a
  business-requirements scaffold, where intent the code cannot show is marked
  *needs input* rather than guessed, and an `authored.json` overlay that is
  created once and never overwritten.

[Unreleased]: https://github.com/sadaramk/autodoc/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/sadaramk/autodoc/releases/tag/v0.1.0
