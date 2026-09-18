---
name: autodoc-engine
description: Generate verifiable, editorial-grade C4 architecture diagrams and interactive documentation from source code.
triggers:
  - "document architecture"
  - "draw system diagram"
  - "explain codebase design"
  - "visualize architecture"
---

# AutoDoc Engine Agent Protocol

You produce architecture diagrams that are **true to the code** and **editorially restrained**.
You never write SVG, Mermaid, or layout coordinates. You reason in `DiagramIR` (typed JSON);
the engine validates it, lays it out deterministically, and renders a standalone HTML page
whose every card links back to the exact file and line range it came from.

Tools come from the `autodoc` MCP server (`autodoc serve`). If MCP isn't connected, use the
CLI equivalents listed at the end; the behaviour is identical.

## When invoked

### 1. Analyze first — never draw from memory

Call `autodoc_scan_repository` with `repoPath` and `depth`:

| The user wants…                                   | `depth`     |
|---------------------------------------------------|-------------|
| "what is this system, who uses it, what does it call" | `system`    |
| services, apps, datastores, queues, vendors       | `container` (default) |
| inside one service (modules and their imports)   | `component` + `focus: "<container id>"` |

Tests, fixtures, examples, docs and scripts are excluded by default; pass `includeTests: true` only when the user
asks about them.

Read from the result:
- `report.containers` — deployable units with `kind`, `techStack`, `entryPoints`
- `report.infrastructure` — datastores, event buses, vendor APIs, with `usedBy` and `topics`
- `report.relationships` — edges with `edgeType`, `label`, `sources` (code / compose / manifest) and evidence
- `report.evidenceMap` — element id → `{filePath, startLine, endLine, symbolName}`
- `draftIr` — a valid starting IR; `draftValidation` — its diagnostics; `draftNotes` — what was merged, grouped or dropped (read these: they tell you what the draft left out)
- units with `techStack` ending in `not parsed` were found from compose/Dockerfiles only — say so in the brief

Relationships backed by `code` are observed; `compose`/`manifest` only are declared. Say which in your brief.

Pick the diagram type for the question, not by habit (details in `reference.md` → *Purpose-built diagrams*):
a **sequence** diagram for "what happens when…", an **entity-relationship** diagram for "what do we store",
a **lifecycle** for "what states can an order be in", a container/component view for "what exists".
With `behavior` scans the report also carries `api` (operations, models, rules, client calls), `data`
(entities, state machines) and `flows` (traced request sequences with per-message evidence) to draft them from.

### 2. Formulate the IR

Start from `draftIr`. Refine; don't rebuild. Rules the compiler enforces:

- **One story.** Set `isKeyFocalPoint: true` on exactly the node the diagram is about (max 2 — `ERR_ACCENT_OVERUSE` above that).
- **One critical path.** Mark `isPrimaryPath: true` on the edges of the main transaction only (client → focal node → system of record). ≤ 6 edges, contiguous.
- **Density ≤ 0.40.** `(nodes + edges) / 80 grid units`, i.e. at most **32 nodes + edges**. Above that, group: collapse sibling nodes into one node representing their container, merge parallel edges, or split into a container view plus a component view.
- **Boundaries carry meaning.** Use `containers` for trust zones and tiers (`client`, `trust-zone`, `internal-service`, `storage`, `third-party`). Don't draw a boundary around one node unless the boundary is the point.
- **Evidence or it didn't happen.** Copy `evidence` from `evidenceMap` for every node you can. Never invent line numbers — the compiler checks them against the repository and the pinned `metadata.commitHash`.
- **Labels are editorial.** Node labels ≤ 32 chars, edge labels ≤ 32 chars, verbs on edges ("writes orders", "publishes order.placed"). Put detail in `metadata.docstring`, not on the canvas.
- **Every node connects.** Orphans are rejected. A cycle needs at least one labelled edge explaining why it feeds back.

### 3. Compile and self-correct

Call `autodoc_compile_diagram` with `ir`, `outputPath` (`.html` or `.svg`), `format`, and `repoPath`.

- `status: "compiled"` → done; note `warnings` and `evidence` counts.
- `status: "rejected"` → nothing was written. For each diagnostic:
  1. Change **only** what `path` / `elementIds` name.
  2. If `patch` is present, apply it (RFC 6902; indices refer to the IR you sent — apply removals highest index first).
  3. Otherwise follow `suggestions[0]`.
  4. Resubmit the whole IR. Stop after 3 rounds and report the remaining diagnostics to the user.

Common fixes:

| Code | Fix |
|------|-----|
| `ERR_HIGH_DENSITY` | Apply the first grouping suggestion; remove peripheral leaves off the primary path |
| `ERR_ACCENT_OVERUSE` | Keep the most connected focal node; express the rest via `isPrimaryPath` |
| `ERR_MISSING_ENDPOINT` | Typo in `source`/`target` — the patch names the closest id |
| `ERR_ORPHAN_NODE` | Connect it to what it talks to, or remove it |
| `ERR_UNLABELED_CYCLE` | Label one edge in the loop, or mark the return leg `async`/`event` |
| `ERR_EVIDENCE_*` | Re-read `evidenceMap`; clamp ranges; fix `symbolName` |
| `WARN_EVIDENCE_STALE` | Code changed since `commitHash`; re-scan before publishing |

Use `autodoc_verify_evidence` when the user asks "is this still accurate?" or before refreshing an old diagram.

### Documenting a whole repository

When the user wants the architecture documented (not one targeted diagram), call `autodoc_generate_book` with
`repoPath` (and `outDir` if they name one). It scans, drafts every view, verifies every citation and writes
`index.html`, Markdown pages, `llms.txt` and `diagrams/*.ir.json`. To improve a figure, edit its
`diagrams/<id>.ir.json` (same rules as above) and run `autodoc_generate_book` again — hand-edited IR is kept and
re-rendered. Use `checkOnly: true` to answer "is the documentation still accurate?".

The book serves several readers: architecture and container pages, an **API reference** per service (parameters,
request/response models, rules, errors, auth, callers), **request flows** as sequence diagrams, the **data model**
(ER figure, columns, lifecycles), a **functional specification** (FR-### per operation, BR-### business rules) and a
**business requirements** scaffold. Flows cover scheduled jobs and message/event handlers as well as requests, and follow events
across services; large APIs get a capability map and access matrix; large data models are split by domain; modular
monoliths get module contracts and boundary violations; declared Compose/Kubernetes environments get a runtime page. Business intent — actors, purpose, goals, stakeholders, metrics — is never
inferred: it comes from `authored.json` in the book directory (created once, never overwritten) and every unanswered
item is shown as *needs input*. When the user tells you intent, write it there; don't put it in IR or page text.

### 4. Deliver

Return the absolute output path (the book's `index.html`, or the compiled diagram), then an executive brief of at most ~150 words:

1. **What the system is** — one sentence.
2. **Critical path** — the primary-path edges, in order.
3. **Boundaries & dependencies** — datastores, queues, vendors; note anything only declared (compose/manifest) rather than observed in code.
4. **Risks or notable choices** — e.g. a shared database written by several services, a synchronous vendor call on the checkout path.
5. **Evidence** — `verified/pinned` counts, and the commit it was pinned to.

## CLI equivalents

```bash
autodoc analyze <repo> --depth container --json scan.json --emit-ir draft.ir.json
autodoc validate draft.ir.json --json          # diagnostics + patches, exit 1 on errors
autodoc render draft.ir.json -o docs/architecture.html --repo <repo>
autodoc verify src/server.ts:18-23 --repo <repo>
autodoc generate <repo>                        # the architecture book (index.html, Markdown, llms.txt, diagrams)
autodoc check <repo>                           # exit 1 when the book is outdated or cites changed code
autodoc schema                                 # DiagramIR JSON Schema
```

See `reference.md` for the IR field guide, visual semantics, and worked examples.
