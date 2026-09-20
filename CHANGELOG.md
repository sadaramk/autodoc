# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0 the
`DiagramIR` schema and the CLI surface may still change between minor versions;
`version` in every IR file says which schema it was written against.

## [Unreleased]

## [0.2.3] - 2026-09-19

### Security

- Repository content can no longer inject Markdown structure into the book. The
  escaper handled `\ * _ <` but not brackets, so a route path carrying
  `](https://…)` closed the link label the renderer had opened and left a live
  link to a destination the repository chose in the operations table. Heading
  text, callout titles, stat labels and table headers were interpolated with no
  escaping at all.

### Fixed

- Kotlin controllers are documented the way Spring reads them. The Kotlin
  extractor is a copy of the Java one and had drifted: a `${…}` placeholder was
  emitted as a literal path segment and the path still called exact,
  `@RequestMapping` without a `method` was reported as GET rather than every
  verb, only the first of a list of verbs or paths was kept, and a `@Controller`
  returning a view name was published as a REST operation. Kotlin writes a
  literal `${…}` as `\${…}`, and the backslash was ending up in the path.
- The book agrees with itself about how much of it is verified. Figure nodes
  carry citations and were registered after the evidence page had counted them,
  so that page reported a smaller total than the README and the manifest — the
  bundled example said 135 of 135 while its manifest said 138 — and a stale
  citation among them never reached the page's warning.

### Internal

- Eleven fixtures that were asserted at model level now also have a book built
  for them, including every Kotlin shape, three ORM shapes, JAX-RS, Micronaut
  and the Kubernetes topology. The information-architecture test is the only one
  that catches a dangling figure id, a citation with no page or a broken anchor,
  and it saw thirteen of twenty-four repositories.

## [0.2.2] - 2026-09-19

A security release. Upgrade if you point autodoc at a repository you did not
write — which is what it is for.

### Security

- **A scanned repository could make git run a command of its choosing.** git
  executes `core.fsmonitor` from a repository's own `.git/config`, so
  `autodoc generate` on a prepared repository was arbitrary code execution as
  the user running it. Every invocation now overrides the configuration keys
  that can execute something, and diffs run `--no-textconv --no-ext-diff`. A
  clone does not carry the source repository's config, so this reached you
  through an archive, a tarball or a vendored copy.
- **Evidence could be read from outside the repository through a symlinked
  directory.** Counting `..` components is not enough — `vendor -> /etc`
  resolves outside while spelling like an ordinary path — and the verifier
  quotes what it finds into the book as a snippet. Resolution must now end
  inside the repository.
- **A repository's own `autodoc.toml` could choose where autodoc writes.** An
  absolute `output.dir` discarded the base path. A configured output directory
  must be relative and inside the repository; `--out` is unrestricted.

### Fixed

- A `// indirect` dependency in a `go.mod` no longer implies architecture. The
  parser stripped the comment before it could read the marker, so transitive
  dependencies became direct ones: Caddy was documented as connecting to
  PostgreSQL and MySQL and MinIO to MongoDB, none of which appears in their
  source.
- A table `CHECK` is attached to the column it names rather than the first one
  whose name is a substring of the expression — `CHECK (valid_until > …)` was
  documented as a constraint on `id`.
- A string is only read as a query when the word after `FROM` names a table, so
  "Select a workspace from the list" no longer produces database read edges.
- An operational route is recognised only when it describes the service, not a
  resource: `/dashboards/{id}/metrics` is functionality and was being dropped
  from the API contract.
- Three slices stepped past a delimiter by one byte, or took a fixed byte
  prefix, and split multibyte characters. One aborted the run; one silently
  discarded the entire data model.

### Changed

- The README is a landing page rather than a manual: 265 lines to 96, leading
  with a generated book. The reference material moved to `HEURISTICS.md` and
  `CONTRIBUTING.md`, and `SECURITY.md` now describes the real trust boundary.

## [0.2.1] - 2026-09-19

### Fixed

- `autodoc check` can pass in CI. A book recorded `HEAD`, so committing the book
  moved the commit it claimed to describe and left it stale from birth: the check
  failed however many times the book was regenerated, and there was no way out of
  the loop. A book now records the last commit that changed something it
  describes, so committing it — or any commit that touches nothing documented —
  leaves the check green, while a change to cited code still turns it red.

## [0.2.0] - 2026-09-19

### Added

- gorilla/mux builder chains register routes: `r.Methods("PUT").Path("/x").HandlerFunc(h)`
  puts the path in its own link rather than in the route call's arguments, so the
  registration read as no route at all. MinIO's entire S3 surface was undocumented
  while its metrics router was not — its book goes from 5 operations to 187. The
  handler is taken from behind its middleware wrapper, `Queries(…)` distinguishes
  routes that share a method and path, and forks that keep the API (MinIO ships
  its own) are recognised. A `Subrouter()` prefix resolves when it can be known
  and the path is reported as partial when it cannot.
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

- A runtime figure is drawn only when something in the environment connects.
  A Compose file of development tooling produced a picture of disconnected
  boxes, and five orphan-node validation errors with it, while the workloads
  table beside it already listed every one with its image, ports and
  configuration.
- An API page whose operations show no authentication now says what that means.
  A column of "none found" down a security-relevant field reads as "these are
  open", and what a gateway, a service mesh or a shared server package applies
  before the request arrives is not visible in the service's own code.
- An operational route is recognised wherever its marker sits in the path, so
  `/v2/metrics/bucket` and `/debug/vars` are no longer documented as
  functionality; and a trailing `health` no longer excludes
  `/patients/{id}/health`, which is a resource, not a probe.
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

[Unreleased]: https://github.com/sadaramk/autodoc/compare/v0.2.3...HEAD
[0.2.3]: https://github.com/sadaramk/autodoc/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/sadaramk/autodoc/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/sadaramk/autodoc/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/sadaramk/autodoc/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/sadaramk/autodoc/releases/tag/v0.1.0
