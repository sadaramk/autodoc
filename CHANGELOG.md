# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0 the
`DiagramIR` schema and the CLI surface may still change between minor versions;
`version` in every IR file says which schema it was written against.

## [Unreleased]

### Fixed

- **A requirement identifier no longer changes when another requirement is
  added.** `FR-001` and `BR-001` were positions in a list, produced by
  `enumerate()`. Adding one endpoint to the first service renumbered five of
  the demo's six requirements — every identifier still existed, and every one
  of them now meant something else. Anything citing `FR-005` in a commit
  message, a ticket or a test name was silently pointing at a different
  requirement.

  An identifier is now derived from what it describes. A requirement is an
  operation, and an operation already had a stable name, so
  `api-gateway:POST /checkout` becomes `FR-api-gateway-post-checkout-bf89`; a
  rule is identified by the statement and the key it was already deduplicated
  on. The digest is not decoration: slugging is lossy, `GET /user/profile` and
  `GET /user/{profile}` reduce to the same text, and disambiguating only on
  collision would have meant adding one operation could change another's
  identifier — the same defect in a new place.

  Identifiers in the example book all change once, and are then stable.

### Fixed

- Minimal-API routes written without a leading slash were not read.
  `app.MapGet("api/items", …)` is as ordinary in ASP.NET Core as
  `app.MapGet("/api/items", …)` — the route is relative to the app root — but
  the extractor required the slash and silently skipped the rest. On
  eShopOnWeb, Microsoft's own reference application, that was **every endpoint
  in its public API**: seven operations and the whole API page for that
  service, absent from the book with nothing saying so. Found by pointing
  nunki at real repositories while investigating #6.

### Added

- **Authored prose can be pinned, and `check` reports it when it rots.**
  `authored.json` is the one thing in a book nunki does not derive, and it was
  the one thing it never checked. Two ways it went stale, both silent. An
  intent keyed to an operation that is later renamed simply stopped appearing:
  the lookup missed, the prose vanished, and nothing said so — `check` now
  fails and names the entry. And prose describing code that moved stayed on the
  page and quietly stopped being true; an operation intent can now carry
  `evidence` — `path:START-END`, the spelling `nunki verify` takes — which
  becomes a citation like any other, verified against the commit.

  Pinning is optional by design. An `authored.json` written before this
  existed keeps working untouched: unpinned prose is published and listed as
  unchecked rather than failing a build on upgrade. An untouched starter file
  reports nothing at all, since every entry in it is blank and a blank claims
  nothing — including when the operation it was generated for disappears.

## [0.4.1] - 2026-09-21

### Changed

- The action's Marketplace listing is named "Nunki Architecture Docs". A
  Marketplace name has to be unique across every action, user and organisation
  on GitHub, and `nunki` is a user account, so the listing could not be
  published under it. This is the listing title only: the action is used by
  repository path (`uses: sadaramk/nunki@v0.4.1`), which has not changed, and
  no workflow needs editing.

## [0.4.0] - 2026-09-21

### Added

- **`comment: true` posts the diff on a pull request.** `nunki diff` always
  produced the Markdown; posting it was twenty lines of workflow that every
  consumer would write and most would get wrong in the same place. The action now
  does it: one comment per pull request found by a hidden marker rather than by
  author, edited in place on every push, and deleted again if a later push makes
  the branch match its base — a branch that added a route and then reverted it
  should not keep claiming the route. An unchanged architecture says nothing at
  all instead of commenting "no changes" on every pull request. `--base` defaults
  to the commit the pull request merges into. Commenting is not a verdict, so it
  does not fail the job unless the caller also asked for `--exit-code`.

- **C# is read.** A tree-sitter grammar for C#, `.csproj` as the module
  layout, and ASP.NET Core contracts: attribute-routed controllers with
  the `[controller]` token expanded, minimal-API `Map*` routes, `[FromQuery]` /
  `[FromRoute]` / `[FromHeader]` / `[FromBody]` binding with `Name =` renames,
  data annotations as validation rules, `[Authorize]` roles and policies with
  `[AllowAnonymous]` overriding them, and the statuses a handler returns or
  throws. Entity Framework Core entities come from the `DbSet<T>` properties,
  `OnModelCreating`'s `ToTable` / `HasColumnName` / `HasKey`, and the annotations
  on the properties — all three, because any one alone gives the wrong table
  name. `_db.Products.Add` and `.FindAsync` are recorded as writes and reads.
  A `.cs` file no longer appears in the unread census.

- A book says on its first page how much of the source it read, and names what
  it could not. Below a tenth read, nunki refuses to write one at all;
  `--allow-partial` overrides that.

- A Homebrew tap: `brew install sadaramk/nunki/nunki`, on macOS and Linux. The
  formula installs the prebuilt binary, so nothing compiles.

  This is the fix for a macOS problem worth naming. The binaries are ad-hoc
  signed — Rust's default, the minimum for arm64 to execute — not notarized. A
  release archive downloaded through a **browser** therefore carries the
  quarantine attribute, and Gatekeeper kills it with exit 137 and no output,
  which reads as a corrupt binary rather than a policy decision. Homebrew and
  `curl` fetch without setting that attribute, so both paths work. Notarization
  would be the other fix, and it costs $99/yr.

  The release workflow bumps the formula on each tag when a `TAP_TOKEN` secret
  is present, and says so in the run summary when it is not, rather than
  failing.

### Fixed

- `outputs.binary` named a path that no longer existed. The entrypoint unpacked
  the binary into a temporary directory it removes on exit, and only got away
  with it because the script ended in `exec`, which skips the trap. The output was
  therefore only ever valid by accident, and any path through the script that did
  not end in `exec` would have handed later steps a path to nothing. The binary is
  now kept outside the directory that gets cleaned.

- The coverage census counts every source file nunki cannot read, not just the
  five languages it half-supports. Counting only those made the number lie in
  exactly the case it exists for: a repository that is 95% C++ reported full
  coverage, because C++ was not on the list. Measured again on real
  repositories, immich drops from a claimed 100% to an honest 40% — its Dart
  mobile app and Svelte web UI were never read, and the book never said so.

## [0.3.0] - 2026-09-20

### Changed

- **Renamed from `autodoc` to `nunki`.** "autodoc" is the name of Sphinx's
  best-known extension and a generic term for a whole category of tools, so the
  project was unfindable by name — the one thing a name has to do. Nunki is the
  star σ Sagittarii; in Sumerian cuneiform NUN.KI writes the name of Eridu, and
  the IAU made it the star's official name in 2016. It is widely called the
  oldest star name still in use, which turns out to be a claim worth checking:
  the name was lost, recovered from tablets, popularised in 1899 by a source
  whose Mesopotamian etymologies are unreliable, and probably belonged to a
  different asterism. A fitting name for a tool about following claims back.

  The binary, the crates, the config file (`nunki.toml`), the MCP tool names and
  the `NUNKI_*` environment variables all follow. GitHub redirects the old
  repository URLs, and the 0.2.x release assets keep their original names.

  Done now because the cost only rises: no listing, no published crates and no
  known users today.

## [0.2.7] - 2026-09-20

### Fixed

- Pinning the action pins the binary. `uses: sadaramk/nunki@v0.2.6` downloaded
  whatever the newest release was, so a workflow that pinned a version did not
  get it — the wrong default for any tool, and the wrong one twice over for this
  one. An exact `vX.Y.Z` ref now selects that release; a moving tag or a branch
  still takes the newest.

### Added

- A moving major tag, so `uses: sadaramk/nunki@v0` tracks the newest 0.x. The
  release moves it.

### Added

- `nunki export --format drawio` writes a `.drawio` file rather than a CSV,
  and is the default. CSV cannot express an edge route or a label position, so
  draw.io re-routed every connector through the boxes and dropped every label on
  its own midpoint, printing them over the node names. The file carries the
  routes and label placements the book already computed. `--format drawio-csv`
  still writes the CSV, for merging into an existing drawing.

### Fixed

- The draw.io export places shapes where nunki's own layout puts them, with
  the boundaries and sizes from the book. It previously left the layout to
  draw.io, which produced crossed edges and labels printed over one another
  because its flow layouts do not respect boundary groups — and its `width` /
  `height` directives were written without the `@` that makes them read a
  column, so every shape was auto-sized to its label. Verified by importing the
  exported file into draw.io.
- Edge labels carry a background, so several edges leaving one service no longer
  print their labels over each other.
- No `link` column is emitted when nothing resolves to a URL, rather than an
  empty link on every shape.

## [0.2.6] - 2026-09-20

### Added

- `nunki export IR --format drawio` writes draw.io's CSV import: real shapes
  with a layout applied, not a flattened image. Every shape carries its
  `file:line` as shape data and links to the line it came from, so a diagram
  pasted into a slide can still be checked. One way on purpose — reading a
  drawing back would mean deciding whether the file or the source is right about
  the architecture, and the source is.
- `nunki diff [PATH] --base REV [--head REV]` reports what changed
  architecturally between two revisions: services, connections, routes, request
  and response shapes, authentication requirements, tables and columns. Markdown
  for a PR comment, `--json` for a bot, `--exit-code` to gate a merge. Both
  revisions are read in throwaway worktrees, so the caller's working tree is
  never touched.
- A request or response whose model keeps its name but changes shape is reported
  by its fields. An inline response literal is given a generated name, so
  comparing names alone said nothing when a field was added.

### Changed

- Workflows use `actions/checkout@v7`, `setup-node@v7`, `upload-artifact@v7` and
  `download-artifact@v8`; the v4 line runs on a deprecated Node.

## [0.2.5] - 2026-09-20

Two themes: the book no longer claims more than it read, and installing nunki
no longer needs a Rust toolchain.

### Added

- Prebuilt binaries for every tag: Linux (musl, static) and macOS on x86_64 and
  arm64, and Windows on x86_64, with a `SHA256SUMS` beside them. Each target
  generates and checks a book before it is published.
- `curl … /releases/latest/download/install.sh | sh` installs one, verifying the
  published checksum first.
- A composite GitHub Action, so CI is `uses: sadaramk/nunki@v0.2.5` instead of
  a two-minute `cargo install`. It downloads the binary for the runner, verifies
  the checksum, and runs any subcommand.
- The API reference names the routes it does not document, with the reason and a
  citation. Spring PetClinic has seventeen mappings and two REST operations; the
  book documented the two under "the contract of every operation this service
  serves" and said nothing about the other fifteen server-rendered views.
- A language census in the scan notes: source files recognised by language but
  with no grammar to read them are counted by language, and a repository that
  was mostly unreadable says so before anything else.

### Fixed

- Go: a bare reassignment is no longer treated as a constant a path can resolve
  to. `p = "/"` inside one function made every `.Post(p, …)` in the module look
  like a route at `/`, so caddy's book documented one operation — `POST /`,
  "handled by `int64`" — which is an outbound FastCGI client call.
- An empty repository is refused rather than documented. Pointed at a directory
  with no manifest, no container file and no source in a language nunki reads,
  it produced a six-page book around an empty diagram and exited 0, which is
  what a mistyped path or a failed checkout looks like.
- A failure is reported once. `EngineError` and `BookError` derived their message
  from an inner error that was also their source, so `cannot scan …` printed
  twice.

## [0.2.4] - 2026-09-19

Claims the analyzer could not support, found by reviewing the analyzer against
real repositories. Each one produced confident output rather than missing
output, which is the failure this project exists to avoid.

### Fixed

- A write is drawn to the store that holds the entity. The step took the unit's
  first storage in enum order, so a service with JPA on Postgres and a
  `@Document` saved through a Mongo repository had its Mongo writes drawn to
  Postgres, with the real Mongo call site cited beside the wrong participant.
- A unit at the repository root no longer answers to every service's name. Its
  configuration prefix is empty and every path starts with the empty string, so
  it collected every `spring.application.name` below it as an alias — and a
  `@FeignClient` naming any of them resolved to the root rather than the service
  that configures it.
- A route under a prefix that could not be resolved is reported as partial.
  `eval_path` already said when it could not resolve a path, and the flag was
  kept at the leaf and dropped at all three prefix sites.
- A drizzle column survives a builder chain wrapped by Prettier: the loop read
  one physical line, so a primary key became an ordinary nullable column, a
  required column became optional, and a foreign key disappeared.
- An exporter, console or admin UI is no longer mistaken for the store it
  watches. Image names match by substring, so `postgres-exporter` was documented
  as a PostgreSQL database and `kafka-ui` as an event bus.
- Every operation and model has its own anchor. `slug` is many-to-one, so
  `/user-profile` and `/user_profile` shared one heading id and every link went
  to the first; a model whose anchor collided was dropped from its page while
  the links to it remained.
- A source file that cannot be read as text is named in the scan notes and on
  the evidence page, rather than disappearing with its routes and entities.

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

A security release. Upgrade if you point nunki at a repository you did not
write — which is what it is for.

### Security

- **A scanned repository could make git run a command of its choosing.** git
  executes `core.fsmonitor` from a repository's own `.git/config`, so
  `nunki generate` on a prepared repository was arbitrary code execution as
  the user running it. Every invocation now overrides the configuration keys
  that can execute something, and diffs run `--no-textconv --no-ext-diff`. A
  clone does not carry the source repository's config, so this reached you
  through an archive, a tarball or a vendored copy.
- **Evidence could be read from outside the repository through a symlinked
  directory.** Counting `..` components is not enough — `vendor -> /etc`
  resolves outside while spelling like an ordinary path — and the verifier
  quotes what it finds into the book as a snippet. Resolution must now end
  inside the repository.
- **A repository's own `nunki.toml` could choose where nunki writes.** An
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

- `nunki check` can pass in CI. A book recorded `HEAD`, so committing the book
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

- **Architecture books.** `nunki generate` writes a self-contained book:
  interactive HTML, a Markdown mirror for GitHub, `llms.txt` for agents, and the
  typed `DiagramIR` behind every figure, which can be edited by hand and kept.
- **Verified evidence.** Every claim links to a file and line, pinned to a
  commit and verified against it. `nunki check` fails when the code has moved
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

[Unreleased]: https://github.com/sadaramk/nunki/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/sadaramk/nunki/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/sadaramk/nunki/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/sadaramk/nunki/compare/v0.2.7...v0.3.0
[0.2.7]: https://github.com/sadaramk/nunki/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/sadaramk/nunki/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/sadaramk/nunki/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/sadaramk/nunki/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/sadaramk/nunki/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/sadaramk/nunki/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/sadaramk/nunki/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/sadaramk/nunki/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/sadaramk/nunki/releases/tag/v0.1.0
