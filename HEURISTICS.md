# Heuristics and limits

How autodoc decides what it decides, and where it stops. For the shorter version aimed at readers of
a book rather than of this repository, see
[what it reads](https://sadaramk.github.io/autodoc/support.html) and
[how it works](https://sadaramk.github.io/autodoc/how.html).

Relationships are inferred statically and labelled with where they came from (`code`, `compose`, `manifest`):

- **Scope.** Test, fixture, example, docs, scripts and tooling directories and test files (`*_test.go`, `*.spec.ts`,
  `test_*.py`, …) are excluded by default; `--include-tests` / `includeTests` scans them too.
- **Units.** Manifests (`Cargo.toml`, `package.json`, `go.mod`, `pyproject.toml`, `requirements.txt`) define units;
  uv/Cargo/npm workspace roots aggregate rather than count. A unit is an HTTP service only with a runnable entry point
  — a library that depends on a web framework stays a library, and a crate other units link against stays a library
  even if it ships a helper binary. Compose services built from languages without a grammar (C#, Java, Kotlin, Ruby,
  PHP, Elixir) still appear, pinned to their Dockerfile.
- **Compose.** Files are discovered at the root and in `docker/`, `deploy/`, `deployment(s)/`, `infra/`, `ops/`.
  Services map to units by build context, Dockerfile directory, or service/image name with the repository prefix and
  role suffixes (`-http`, `-grpc`, `-api`, …) removed. Addresses from a shared `env_file` only become edges when the
  service's code — or a workspace library function it calls — reads that variable.
- **Calls.** HTTP/gRPC edges need a shared hostname or `<SERVICE>_URL`-style reference (compose environment, URL
  literals such as `'http://ml:3003'`, `process.env`/`os.Getenv`/`os.environ`); dynamic service discovery isn't
  visible. Web clients link to the one obvious backend when their code references an API base URL.
- **Datastores.** Reads vs writes come from SQL literals (`INSERT`/`UPDATE`/`DELETE` vs `SELECT`), ignoring
  migrations; ORMs (SQLModel, SQLAlchemy, TypeORM, GORM, …) show as `queries`; Redis commands are counted only on a
  cache-like receiver.
- **Events.** Producers/consumers come from call names (`publish`, `send` on a producer, `subscribe`, `consume`,
  `run` on a consumer) in files that import a known messaging client.
- **JVM (Java, Kotlin).** Maven (`pom.xml`, aggregators with `<modules>` aren't units) and Gradle (`build.gradle[.kts]`,
  `project(':x')` links) modules are units; sibling modules link by artifactId when an import confirms it. A module is
  a deployable only with an application object (`@SpringBootApplication`, `SpringApplication.run`, `Micronaut.run`,
  `@QuarkusMain`, or a container-managed `@Path` resource for Quarkus / JAX-RS); shared modules holding controllers
  or `@KafkaListener`s stay libraries, and a library's datastore and queue use is drawn on the deployables that link
  it. Services are known by `spring.application.name` (also Micronaut/Quarkus), which resolves `@FeignClient`,
  MicroProfile `@RegisterRestClient` and Micronaut `@Client` targets and compose `image:` names; Zuul and Spring Cloud
  Gateway routes are read from application config, including Spring Cloud Config `shared/<app>.yml` files (marked
  *declared in config*). Starters and drivers (JPA/JDBC drivers, Data MongoDB/Redis/Elasticsearch, spring-kafka,
  AMQP, AWS SDK, …) and imports name the infrastructure; `KafkaTemplate`/`RabbitTemplate` sends publish and
  `@KafkaListener`/`@RabbitListener`/`@JmsListener`/`@Incoming` consume. Config servers, registries and dashboards are
  platform services: when most services depend on one it is left off the container figure. Component views group
  packages below the application's base package (Spring Modulith modules, named by `package-info.java`), and
  `publishEvent(new X(…))` → `@ApplicationModuleListener`/`@EventListener` becomes an event edge. `com.example`-style
  packages are never mistaken for example directories. Kotlin is parsed too (Spring MVC/WebFlux incl. `coRouter`, Ktor routing and `HttpClient`, JPA, Spring Data, Exposed, `@FeignClient`); Groovy and `.kts` build scripts aren't parsed.
- **Large workspaces.** More than five linked libraries are grouped by parent directory ("DB Views (24 crates)"),
  transitive `uses` edges are pruned, and density compaction removes libraries before infrastructure and never a
  deployable on the critical path.
- **Text** width is estimated from glyph classes, not measured from fonts; long labels are truncated with the full
  text in the inspector and tooltip.

## Validation

| Guardrail | Rule | Code |
|-----------|------|------|
| Density | `(nodes + edges) / 80 ≤ 0.40` → at most 32 elements | `ERR_HIGH_DENSITY` + grouping suggestions |
| Colour budget | `isKeyFocalPoint` on ≤ 2 nodes | `ERR_ACCENT_OVERUSE` + patch |
| Topology | endpoints exist, no orphans, no self-loops, cycles carry a label | `ERR_MISSING_ENDPOINT`, `ERR_ORPHAN_NODE`, `ERR_SELF_LOOP`, `ERR_UNLABELED_CYCLE` |
| Evidence | file exists, lines in range, symbol in range, unchanged since the pinned commit | `ERR_EVIDENCE_*`, `WARN_EVIDENCE_STALE` |

The density grid is fixed (10 × 8 cells of a 16:10 canvas) so scores are comparable across diagrams.

## Layout

The renderer is deterministic and dependency-free. Containers become blocks ranked left→right along the flow
(cycle-broken longest path) and ordered by barycentric sweeps; busy containers spread into sub-columns by internal
rank. Every connector is orthogonal with quarter-arc corners: it leaves a card into the gutter beside its column,
runs on its own track, and crosses intermediate columns only through free lanes between blocks. Attach points fan
out along card edges, labels sit on masks clear of every card and their own stroke, and lower-priority lines hop
over crossings. These properties are asserted by `Layout::check_geometry` on every fixture and on 300 seeded random
graphs.

## Validation on public repositories

Besides the fixtures, the analyzer is exercised against real repositories of different shapes. Each finding below
became a fix plus a regression fixture in `tests/fixtures/real-world/`.

| Repository (shape) | Result | What it exposed |
|--------------------|--------|-----------------|
| dockersamples/example-voting-app (compose; Python, Node, C#) | vote, result, worker (C#, not parsed); Redis and Postgres edges | unparsed-language services vanished; `request.cookies.get` counted as a Redis read |
| fastapi/full-stack-fastapi-template (uv workspace; FastAPI + React) | backend → Postgres (`queries`), frontend → backend | script entry chosen over `FastAPI()`; Alembic migrations counted as writes; a Python `emails` dependency linked to an npm package named `emails`; root-context Dockerfile builds unmapped |
| ThreeDotsLabs/wild-workouts-go-ddd-example (Go services, gRPC, shared `.env`) | trainings → trainer and trainings → users over gRPC; Firestore and MySQL | `env_file` ignored; `trainer-grpc` not mapped to `trainer`; a library using `chi` classified as a service |
| immich-app/immich (TypeScript monorepo + Python ML, compose in `docker/`) | web → server → machine-learning; Postgres, Redis | compose outside the root missed; docs and e2e helper packages shown as containers; Vite-only packages classified as web clients; README subtitle picked up badge HTML |
| LemmyNet/lemmy (40-crate Rust workspace) | server → grouped crates → Postgres | libraries depending on Actix classified as services; density compaction dropped the `server` binary; transitive pruning over grouped-library cycles orphaned it |

Scans took 0.1–0.8 s (debug build) for 90–750 parsed files.

