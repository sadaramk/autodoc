# Contributing

Thanks for considering a contribution. This project documents other people's code, so its
own bar is: **everything it prints must be traceable to a line of source, and every change must be
covered by a test that reads that line back.**

## Getting set up

Everything runs in Docker; nothing but Docker (or Podman with a Docker socket) is required.

```bash
make test        # Rust unit + integration + CLI/MCP journeys, TypeScript contract tests, browser journeys
make lint        # rustfmt + clippy (-D warnings)
make demo        # regenerate examples/polyglot-shop
```

To iterate faster you can use a local Rust toolchain (`cargo test`, `cargo clippy --workspace
--all-targets -- -D warnings`) and Node 24 for `packages/ir-spec-ts`, but the Docker suites are the
ones that gate a merge.

## What a good change looks like

- **Evidence or it didn't happen.** A new fact (a route, an entity, a flow step, a workload) carries
  an `EvidenceRef` to the exact file and line that states it. Tests assert the cited line contains
  what the fact claims — see `crates/analyzer/tests/real_world_shapes.rs` for the pattern.
- **Never infer intent.** Business purpose, actors and goals come from `authored.json`, never from
  heuristics. If the code doesn't say it, the book shows a gap.
- **Add a fixture.** Behaviour changes come with a small repository under `tests/fixtures/` (each
  fixture has a README row explaining what it exercises), not with a change to an unrelated one.
- **Check a real repository.** For analyzer heuristics, run `autodoc generate` against a public
  repository in the language you touched and say in the PR what you verified by reading its source.
- **Keep the diagrams honest.** Figures are compiled from typed `DiagramIR`; the validator enforces
  density, accents and evidence. If a figure needs a new shape, extend the IR and its JSON Schema and
  the zod mirror together (`make schema`, `packages/ir-spec-ts`).

## Pull requests

1. Branch from `main`.
2. `make lint && make test` must pass; run `make demo` if analyzer or book output changed and commit
   the regenerated example.
3. Describe what you verified against real code, not only that tests pass.
4. Keep commits focused; don't mix a refactor with a behaviour change.

## Reporting a bug

The most useful report names a public repository and what the documentation got wrong about it:
what it says, what the code actually says, and the file and line that proves it.

## Repository layout

```
crates/
  ir-spec/        DiagramIR types, parser with JSON-path errors, JSON Schema, density math
  git-context/    commit pinning, diff hunks, evidence verification, forge permalinks
  analyzer/       tree-sitter extraction, manifests, compose, C4 model, component graphs, draft IR
  validator/      semantic checks, density guardrail, self-healing diagnostics, JSON Patch
  renderer/       layout engine, editorial SVG, interactive HTML (vanilla JS)
  book/           architecture book: page model, citations, HTML reader, Markdown, llms.txt, check
  mcp-server/     engine facade + MCP stdio server (`autodoc-mcp` binary)
  cli/            `autodoc` binary
packages/ir-spec-ts/   zod mirror of DiagramIR
schema/                committed JSON Schema
skills/autodoc/        SKILL.md + reference.md
tests/
  contract/            valid/invalid IR fixtures shared by Rust and TypeScript
  fixtures/            polyglot-shop, single-language minis, real-world/ shapes from public-repo testing
  e2e/                 Playwright journeys over the generated HTML
examples/polyglot-shop generated demonstration book
```

## Testing

All suites run in Docker:

```bash
make test        # Rust (unit, integration, CLI + MCP journeys), TypeScript contract tests, Playwright journeys
make lint        # rustfmt + clippy -D warnings
make demo        # regenerate the examples/polyglot-shop book
```

## The architecture book

`autodoc generate` writes the same structure for every repository:

```
docs/architecture/
  index.html              the book: self-contained, offline, light/dark
  manifest.json           commit, pages, figures, evidence health, content hashes
  README.md  pages/*.md   Markdown mirror (GitHub, Obsidian, docs sites)
  llms.txt  llms-full.txt for agents
  diagrams/<id>.ir.json   typed DiagramIR — edit by hand; your edits are kept
  diagrams/<id>.svg       standalone figures for embedding
```

| Page | What it answers |
|------|-----------------|
| **Overview** | What the system is (cited to the README), at-a-glance stats, system context figure |
| **Architecture** | Container figure; every deployable with its entry point; every relationship marked *observed in code* or *declared* in compose/manifests; shared libraries |
| **Containers → one page per deployable** | Role, technology, entry points, component figure, modules, what it depends on and what uses it, key symbols |
| **Data & integrations** | Service × datastore access matrix (reads / writes / queries), topics with publishers and consumers, external APIs |
| **Data & integrations** (cont.) | Data model: ER figure with keys and cardinality, a column table per entity (types, defaults, constraints, who writes/reads it), and a lifecycle figure per state machine with its guarded transitions. Models over 12 entities split by domain (package / service / FK component): an overview figure with cross-domain references and one ER figure per domain (own pages above 40 entities) |
| **Critical flows** | The primary path as a step-through walkthrough; request flows as sequence diagrams traced from each handler; **scheduled jobs** (`@Scheduled`, cron libraries, tickers) and **message & event handlers** (Kafka/RabbitMQ consumers, Celery, BullMQ, `@EventListener`, …) traced the same way; a published event continues into its consumers' traces across services and languages (topic-matched, marked asynchronous). Every message cited |
| **API reference → one page per service** | Contract coverage (typed / partial / opaque), a capability map (callers → capability groups → what they touch), every operation with wire-named parameters, request and response models with rules, errors, auth, callers and response-field drift; an access matrix (roles, scopes, authenticated, conditions, "no auth found" — never "public"). Services over 25 operations get an overview plus one page per capability group; a combined Access control page when several services enforce auth |
| **Runtime & deployment** | Each declared environment: Compose (base file plus overlay variants) and Kubernetes manifests (Deployments, StatefulSets, CronJobs, Services, Ingress); a figure of how traffic enters, what starts after what and which workloads connect via environment variables; workloads with image/build, ports, replicas, health checks, limits, volumes; Helm templates reported as not rendered |
| **Functional specification** | FR-### per operation and per background flow: actor, purpose, capability, trigger, accepts, must-satisfy rules, state changes, calls, events, returns; the BR-### business rules catalog. Large systems split per service / capability, rules per kind |
| **Business requirements** | Problem, goals, stakeholders, success metrics, assumptions from `authored.json`; scope and capabilities, rules and integrations measured from code; open questions raised by gaps |
| **Evidence & unknowns** | Citation health against the commit, declared-only relationships, unparsed services, what static analysis can't see, citation index |

In the reader, clicking a card in a figure opens that container's page; hovering traces its upstream and downstream
path; every `file:12–30` chip opens the verified snippet with a permalink. The structure comes from the scan, not from
an LLM, so two runs on the same commit produce byte-identical files (only `manifest.json` records the generation
time), a removed service's page is deleted, and `autodoc check` can gate CI.

### Measured vs authored

Code says what the system does, not who it is for or why. `autodoc generate` creates `authored.json` next to the
book once, listing every operation it found, and never overwrites or prunes it. Answers written there (operation
names, actors, purposes, acceptance criteria; the problem, goals, stakeholders and metrics) appear badged
*authored*; anything left empty is shown as *needs input* — never filled in by inference.

## MCP

```bash
claude mcp add autodoc -- autodoc serve          # Claude Code
```

```json
{ "mcpServers": { "autodoc": { "command": "autodoc", "args": ["serve"] } } }   // Cursor and others
```

| Tool | Input | Output |
|------|-------|--------|
| `autodoc_scan_repository` | `repoPath`, `depth`, `focus?`, `theme?`, `includeTests?` | C4 report (containers, infrastructure, relationships, entry points), `evidenceMap`, validated `draftIr` |
| `autodoc_compile_diagram` | `ir`, `outputPath`, `format`, `repoPath?`, `accent?`, `strict?` | `status: compiled` with path, density and evidence counts — or `status: rejected` with diagnostics (`isError: true`) |
| `autodoc_generate_book` | `repoPath`, `outDir?`, `accent?`, `theme?`, `includeTests?`, `checkOnly?` | pages/figures written, evidence health, kept hand-edited diagrams; or the `check` report |
| `autodoc_verify_evidence` | `evidenceList[{filePath, line, endLine?, symbolName?}]`, `repoPath?`, `commitHash?` | per-item state (`verified`, `stale`, `untracked`, `file-missing`, `line-out-of-range`, `symbol-mismatch`) with permalinks |

The agent protocol lives in [`skills/autodoc/SKILL.md`](skills/autodoc/SKILL.md); the IR field guide, diagnostic
catalog and worked examples in [`skills/autodoc/reference.md`](skills/autodoc/reference.md).
To install the skill for Claude Code: `cp -r skills/autodoc ~/.claude/skills/autodoc`.

