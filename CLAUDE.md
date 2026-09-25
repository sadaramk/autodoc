# nunki

Rust workspace (`crates/*`), a TypeScript zod mirror (`packages/ir-spec-ts`), Playwright journeys (`tests/e2e`).

## Running tests (Docker)

| Layer | Command |
|-------|---------|
| Rust unit + integration + CLI/MCP journeys | `docker compose run --rm test` |
| TypeScript contract tests | `docker compose run --rm test-ts` |
| Browser journeys (built release binary) | `make e2e` |
| fmt + clippy | `docker compose run --rm lint` |

**Locally, run what the change touches; let CI run the whole matrix.** The
workspace suite links ~30 test binaries and the container shares this machine's
memory with whatever else is running — a full `cargo test --workspace` is where
builds get OOM-killed, and `-j 1` makes it slow rather than reliable. Prefer:

```bash
docker compose run --rm test cargo test -p nunki-analyzer --test flows
docker compose run --rm lint          # always cheap, always before pushing
```

CI runs fmt, clippy, the full workspace suite, the zod contract tests, Playwright
and the example check on every push, on GitHub-hosted runners that are free and
unlimited for this public repository. It is the gate; a local full run is not.

Two things are worth doing locally before pushing regardless, because CI cannot
tell you what they mean: `make demo-check` when analyzer or book output changes,
and reverting your fix to confirm the new test actually fails without it.

`make demo` builds its binary in the `test` service, which bind-mounts this tree.
Do not move it back into an image built with `COPY`: BuildKit normalises mtimes,
cargo compares them against a cached `target/` and skips the build, and the demo
then regenerates the example with the previous binary while `demo-check` passes
against it. Both targets would agree, and both would be wrong (#62).

## Cutting a release

A tag builds and publishes; nothing is uploaded by hand.

1. Move `## [Unreleased]` in `CHANGELOG.md` down to `## [x.y.z] - <date>` and add the
   compare link at the bottom. The release workflow reads that section for the notes
   and **fails** if it is missing.
2. Bump `version` in the root `Cargo.toml`, then `cargo build` so `Cargo.lock` follows
   (the release builds with `--locked`).
3. `make demo` — the generator version is embedded in the examples, so they change.
4. Bump the `sadaramk/nunki@vx.y.z` references in `README.md`, `docs/start.html` **and
   `action.yml`** — the `version` input documents itself with an example tag. Do this every
   release, not only when a snippet uses a new subcommand: an action reads its inputs from the
   ref you pin, so a snippet pointing at an older tag than the input it demonstrates fails
   outright rather than degrading. Leave statements of *when* something landed
   ("`comment` was added in 0.4.0") alone — those are history, not pins.
5. Commit, `git tag -a vx.y.z`, push both. `release.yml` builds five targets, each of
   which generates and checks a book before it is uploaded, and attaches
   `SHA256SUMS` and `install.sh`.

The action in `action.yml` downloads the **last release**, not the current commit, so
the CI job that exercises it must only use subcommands that release already has.

## Critical journeys

Each must pass before merge.

1. **Generate the architecture book** — `nunki generate` → book with verified citations and permalinks; unchanged
   commit rewrites nothing; hand-edited IR kept; `nunki check` fails on drift.
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

- `ir-spec` field changes: regenerate `schema/diagram-ir.schema.json` (`nunki schema`), update
  `packages/ir-spec-ts/src/index.ts`, run `test` and `test-ts` (contract fixtures in `tests/contract`).
  `crates/ir-spec/tests/stability.rs` then refuses a **breaking** change against the frozen baseline
  in `schema/stable/` — additive is free, the rest is a decision. See CONTRIBUTING.md.
- CLI subcommands or flags: `crates/cli/tests/surface.rs` snapshots every `--help`. A diff there is
  the promise in #9 changing, not a chore; regenerate with `UPDATE_CLI_SURFACE=1` once it is a decision.
- `renderer` layout: `crates/renderer/tests/geometry.rs` (fixtures, 300 random graphs, random sequence / ER /
  lifecycle diagrams) and `make e2e`.
- `renderer/src/assets/*.js|css`: `make e2e`.
- `book` pages or `analyzer` output shape: `crates/book/tests/book.rs`, journey 1, `make e2e`, then `make demo`.
- `analyzer` behaviour model (`api`, `data`, `trace`, `views`): `crates/analyzer/tests/api_contracts.rs`,
  `data_model.rs`, journey 5, then `make demo`.
- JVM support (`extract.rs` Java, `manifest.rs` Maven/Gradle, `jvm.rs`, `catalog.rs`): `real_world_shapes.rs::spring_*`,
  `api_contracts.rs`, `data_model.rs`, then spot-check piggymetrics / thingsboard / spring-modulith examples.
  `catalog.rs`'s `NAMESPACE_*` tables are shared with C#, so a change there moves both.
- C# support (`extract.rs` `visit_csharp`, `api/cs.rs`, `data/csharp.rs`, `manifest.rs` `.csproj`,
  `catalog.rs` NuGet and namespaces): `api_contracts.rs::csharp_*`, `data_model.rs::csharp_*`,
  `real_world_shapes.rs::a_service_in_an_unread_language_*`.
- The action (`action.yml`, `scripts/action-*.sh`): `crates/cli/tests/action_comment.rs` and
  `shellcheck -S style scripts/*.sh`. The comment path talks to the GitHub API, which a stub cannot
  prove, so the `action` CI job runs it for real on every pull request.
- `analyzer` heuristics: `crates/analyzer/tests/fixtures.rs`, journeys 1–4, then `make demo` to refresh examples.
- `diff` / `export`: `crates/cli/tests/journeys.rs::journey_diff_*`, `journey_export_*`. Both compare
  against real output, so a wrong-but-plausible change shows up as a changed string, not a panic. The
  draw.io side also has `geometry.rs::drawio_export_does_not_hand_draw_io_a_live_tag`: every shape sets
  `html=1`, so a label needs HTML escaping *before* the format's own — twice for XML, once for CSV,
  because the XML parser undoes a round. Assert per format; asserting the same of both calls one a bug.
- Escaping (`renderer/src/svg.rs` `esc`, `renderer/src/text.rs`, `book/src/html.rs`): a book is rendered
  from an untrusted repository and the reader assigns `d.svg` with `innerHTML`, so
  `crates/book/tests/escaping.rs` scans a repository whose **directory name** is markup — that is what
  becomes a node label — and `geometry.rs::html_is_self_contained_and_escapes_untrusted_text` covers the
  standalone page. Markdown output is deliberately not escaped: it quotes source verbatim.
- Multi-repository support (`scan.rs` `resolve_against_members` / `adopted`, `Evidence.repo`,
  `build.rs` member contexts, `[workspace] members`): `crates/analyzer/tests/multi_repo.rs`,
  `crates/book/tests/multi_repo.rs`, `crates/cli/tests/multi_repo.rs`. The journey test is the only
  one that proves per-repository permalinks and `check` naming a member that moved, because both
  need two real git repositories. `stamp_repo` finds evidence by its serialized shape, so
  `scan.rs::stamp_repo_recognises_evidence` fails if that shape changes — do not delete it.
