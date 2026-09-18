# AutoDoc Engine — Reference

## Principles

1. **The IR is the contract.** Agents emit `DiagramIR`; compiler passes own validation, layout and
   pixels. The same IR always produces byte-identical output.
2. **Deletion is the highest-quality move.** Two nodes that always travel together are one node.
   A connection obvious from layout is not drawn. Target density is 4/10, enforced, not advised.
3. **Accent is editorial, not a status system.** One hue, on 1–2 focal nodes and one critical
   path. Everything else is neutral slate.
4. **Every claim is pinned.** Nodes carry file + line evidence, verified against the working tree
   and the pinned commit. Drift is reported, never silently rendered as truth.
5. **Failures teach.** A rejected IR returns stable codes, JSON paths, ranked suggestions and
   JSON Patches, so an agent can converge in one or two rounds.

## C4 levels

| `diagramType`    | Nodes are…                                   | Boundaries (`containers`) are…            |
|------------------|----------------------------------------------|-------------------------------------------|
| `system-context` | the system, its clients, external systems    | client / system trust zone / third party   |
| `container`      | deployable units, datastores, queues, vendors | clients, platform trust zone, data, third party |
| `component`      | modules inside one deployable unit            | that unit, plus data and third party       |
| `data-flow`      | stages a payload passes through               | ownership or trust zones                   |
| `lifecycle`      | states of one entity                          | phases                                     |

## Field guide

### Top level

| Field | Notes |
|-------|-------|
| `version` | `"1.1.0"` (required for the fields in *Purpose-built diagrams*); `"1.0.0"` still accepted. |
| `diagramType` | See table above. Controls the eyebrow label. |
| `title` | ≤ 80 chars. Name the subject, not the chart type ("Checkout platform — containers"). |
| `subtitle` | One sentence of context. |
| `theme` | `editorial-light` or `editorial-dark`. The HTML page can toggle live. |
| `metadata.targetRepo` | Repository root used to verify evidence when `repoPath` isn't passed. |
| `metadata.commitHash` | Pin. Evidence is diffed against it; changed lines become `WARN_EVIDENCE_STALE`. |
| `metadata.visualDensityScore` | Advisory; the validator recomputes it and warns on mismatch. |

### `containers[]` (boundaries)

| `boundaryType` | Drawn as | Use for |
|----------------|----------|---------|
| `client` | open outline | browsers, mobile apps, CLIs used by people |
| `trust-zone` | dashed outline, faint wash | what you deploy and secure |
| `internal-service` | solid outline, faint wash | one service's internals (component views) |
| `storage` | solid outline; cards recessed | databases, caches, queues, buckets |
| `third-party` | dotted outline; cards dashed | vendor APIs outside your control |

### `nodes[]`

| Field | Notes |
|-------|-------|
| `id` | `[A-Za-z0-9][A-Za-z0-9._:-]*`, unique across containers, nodes and edges. |
| `containerId` | Optional; must reference a declared container. |
| `label` | ≤ 32 chars, sentence case. |
| `subtitle` | Role in ≤ 64 chars ("HTTP service", "Relational database"). |
| `techStack` | Monospaced badge, ≤ 40 chars ("Go · net/http"). |
| `isKeyFocalPoint` | Required boolean. ≤ 2 true; ideally exactly 1. |
| `evidence` | `{filePath, startLine, endLine, symbolName?}` — repo-relative, 1-based, inclusive. |
| `metadata` | String map shown in the inspector. `docstring` gets its own section. |

### `edges[]`

| `edgeType` | Meaning | Default stroke |
|------------|---------|----------------|
| `sync` | request/response call | solid, filled arrow |
| `async` | fire-and-forget, callback | dashed |
| `event` | publish/subscribe delivery | dotted, dot at origin |
| `read` | reads from a store | thin solid, open arrow |
| `write` | writes to a store | heavier solid, filled arrow |

Direction follows data or control: `service → database` for both reads and writes;
`bus → consumer` for event delivery. `style` overrides the dash; `isPrimaryPath` applies the accent.

## Purpose-built diagrams

Pick the view the section needs, not the same boxes every time:

| `diagramType` | Answers | Rendered as |
|---------------|---------|-------------|
| `system-context` / `container` / `component` | what exists and what talks to what | layered cards and orthogonal connectors |
| `data-flow` | where data moves | same, reads/writes emphasised |
| `sequence` | what happens, in order, for one request | participants with lifelines; numbered messages top to bottom |
| `entity-relationship` | what is stored and how it relates | entity cards with attribute rows; crow's-foot cardinality |
| `lifecycle` | which states a thing moves through, and when | state pills; ● initial, double border terminal; `event [guard]` labels |

| Field | Applies to | Notes |
|-------|-----------|-------|
| `edges[].sequence` | sequence | 1-based order, unique. Missing → `ERR_SEQUENCE_MISSING` (patch numbers it). |
| `edges[].reply` | sequence | Return message: dashed, open arrow. |
| `edges[].payload` | sequence | Request/response type shown under the label ("CheckoutRequest"). |
| `edges[].evidence` | all | Pins a message, relationship or transition to code; verified like node evidence and opened from the drawer. |
| `nodes[].attributes[]` | entity-relationship | `{name, typeName, key?: pk \| fk \| pk-fk, nullable?, note?}`; ≤ 16 shown. |
| `edges[].cardinality` | entity-relationship | `1:1`, `1:n`, `n:1`, `n:m` (source:target). Draw parent → child. |
| `nodes[].stateKind` | lifecycle | `initial`, `normal`, `terminal`. A terminal state with outgoing transitions is an error. |
| `edges[].guard` | lifecycle | Condition checked before the transition, as written in code. |

Type rules: sequence diagrams allow self-messages, skip cycle and density checks, and cap at 8 participants and
30 messages; lifecycles allow self-transitions and warn on unreachable states or no initial state; entity diagrams
allow unrelated tables (no orphan error) and warn on relationships without cardinality. A type-specific field on the
wrong diagram type is ignored with `WARN_FIELD_IGNORED`.

## Density

```
density = (nodes + edges) / 80        # 10 × 8 cells of the 16:10 editorial canvas
budget  = floor(0.40 × 80) = 32       # nodes + edges
```

The grid is fixed so a score means the same thing everywhere. `ERR_HIGH_DENSITY` reports the
`excess` and ranked remedies:

1. **Collapse a container** — `k` sibling nodes → 1 node; internal edges disappear and parallel
   external edges merge.
2. **Group equivalent nodes** — nodes with identical neighbourhoods are one concept drawn twice.
3. **Drop peripheral leaves** — degree ≤ 1, not focal, off the primary path.
4. **Split the view** — a container overview plus a component view of the busiest unit.

## Diagnostic catalog

| Code | Severity | Trigger |
|------|----------|---------|
| `ERR_SCHEMA` | error | JSON doesn't match the schema; `path` names the field |
| `ERR_EMPTY_DIAGRAM` / `ERR_EMPTY_FIELD` | error | no nodes / blank title or label |
| `ERR_INVALID_ID` / `ERR_DUPLICATE_ID` | error | unsafe or repeated id |
| `ERR_UNKNOWN_CONTAINER` | error | `containerId` not declared (patch: closest id or remove) |
| `ERR_MISSING_ENDPOINT` | error | edge endpoint isn't a node (patch: closest id or remove edge) |
| `ERR_SELF_LOOP` | error | `source == target` |
| `ERR_ORPHAN_NODE` | error | node with no edges in a multi-node diagram |
| `ERR_UNLABELED_CYCLE` | error | strongly connected nodes with no labelled edge |
| `ERR_HIGH_DENSITY` | error | `(nodes + edges) / 80 > 0.40` |
| `ERR_ACCENT_OVERUSE` | error | more than 2 focal nodes (patch demotes all but the most connected) |
| `ERR_EVIDENCE_RANGE` | error | `startLine < 1` or `endLine < startLine` |
| `ERR_EVIDENCE_FILE_MISSING` / `_OUT_OF_RANGE` / `_SYMBOL_MISMATCH` / `_OUTSIDE_REPO` | error | evidence doesn't hold |
| `WARN_EVIDENCE_STALE` / `_UNTRACKED` / `_UNVERIFIED` | warning | lines changed since the pin / file not committed / no repo available |
| `WARN_COMMIT_MISMATCH` | warning | pinned commit ≠ HEAD |
| `WARN_DUPLICATE_EDGE` / `WARN_EMPTY_CONTAINER` | warning | redundant ink |
| `WARN_NO_FOCAL_POINT` / `WARN_FOCAL_WITHOUT_EVIDENCE` | warning | story without a subject / unpinned subject |
| `WARN_PRIMARY_PATH_OVERUSE` / `_BROKEN` | warning | more than 6 (or half the) edges, or disconnected segments |
| `WARN_LABEL_TOO_LONG` / `WARN_DENSITY_MISMATCH` | warning | will be truncated / declared score is wrong |
| `ERR_SEQUENCE_MISSING` / `ERR_SEQUENCE_DUPLICATE` | error | sequence message without / with a repeated order |
| `ERR_TOO_MANY_PARTICIPANTS` / `ERR_TOO_MANY_MESSAGES` | error | more than 8 participants / 30 messages: split the flow |
| `ERR_TERMINAL_HAS_TRANSITIONS` | error | a `terminal` state has outgoing edges (patch: `normal`) |
| `WARN_NO_INITIAL_STATE` / `WARN_UNREACHABLE_STATE` / `WARN_UNLABELED_TRANSITION` | warning | lifecycle gaps |
| `WARN_MISSING_CARDINALITY` / `WARN_ENTITY_WITHOUT_ATTRIBUTES` / `WARN_TOO_MANY_ATTRIBUTES` | warning | entity diagram gaps |
| `WARN_FIELD_IGNORED` | warning | a type-specific field on another diagram type |

`--strict` (CLI) or `strict: true` (MCP) promotes warnings to errors.

## Example: container view

```json
{
  "version": "1.0.0",
  "diagramType": "container",
  "title": "Checkout platform — containers",
  "subtitle": "How an order moves from the storefront to payment and fulfilment",
  "theme": "editorial-light",
  "metadata": { "targetRepo": "/repo", "commitHash": "4f1c2e9a…", "generatedAt": "2026-09-16T12:00:00Z" },
  "containers": [
    { "id": "clients", "label": "Clients", "boundaryType": "client" },
    { "id": "platform", "label": "Shop platform", "boundaryType": "trust-zone" },
    { "id": "data", "label": "Data & messaging", "boundaryType": "storage" },
    { "id": "external", "label": "Third-party services", "boundaryType": "third-party" }
  ],
  "nodes": [
    { "id": "web", "containerId": "clients", "label": "Web", "subtitle": "Web client", "techStack": "TypeScript · React", "isKeyFocalPoint": false,
      "evidence": { "filePath": "web/src/main.tsx", "startLine": 9, "endLine": 9, "symbolName": "createRoot" } },
    { "id": "api-gateway", "containerId": "platform", "label": "API Gateway", "subtitle": "HTTP service", "techStack": "TypeScript · Express", "isKeyFocalPoint": true,
      "evidence": { "filePath": "api-gateway/src/server.ts", "startLine": 18, "endLine": 23, "symbolName": "main" } },
    { "id": "payments", "containerId": "platform", "label": "Payments", "subtitle": "HTTP service", "techStack": "Go · net/http", "isKeyFocalPoint": false,
      "evidence": { "filePath": "payments/cmd/payments/main.go", "startLine": 17, "endLine": 30, "symbolName": "main" } },
    { "id": "postgres", "containerId": "data", "label": "PostgreSQL", "subtitle": "Relational database", "isKeyFocalPoint": false },
    { "id": "stripe", "containerId": "external", "label": "Stripe", "subtitle": "Payments API", "isKeyFocalPoint": false }
  ],
  "edges": [
    { "id": "web--api-gateway", "source": "web", "target": "api-gateway", "label": "HTTP", "edgeType": "sync", "isPrimaryPath": true },
    { "id": "api-gateway--payments", "source": "api-gateway", "target": "payments", "label": "charge order", "edgeType": "sync", "isPrimaryPath": true },
    { "id": "payments--stripe", "source": "payments", "target": "stripe", "label": "charges cards", "edgeType": "sync", "isPrimaryPath": true },
    { "id": "api-gateway--postgres", "source": "api-gateway", "target": "postgres", "label": "writes orders", "edgeType": "write" },
    { "id": "payments--postgres", "source": "payments", "target": "postgres", "label": "writes payments", "edgeType": "write" }
  ]
}
```

5 nodes + 5 edges → density 0.125. One focal node, one three-edge critical path.

## Example: self-correction round

Submitted: three focal nodes and a typo.

```json
{
  "status": "rejected",
  "validation": {
    "valid": false,
    "diagnostics": [
      { "code": "ERR_ACCENT_OVERUSE", "path": "$.nodes",
        "message": "3 nodes set isKeyFocalPoint; the accent budget allows at most 2 (ideally 1)",
        "suggestions": ["Keep `api-gateway` (most connected) as the single focal point and set isKeyFocalPoint=false on [payments, web]."],
        "patch": [{ "op": "replace", "path": "/nodes/2/isKeyFocalPoint", "value": false },
                  { "op": "replace", "path": "/nodes/0/isKeyFocalPoint", "value": false }] },
      { "code": "ERR_MISSING_ENDPOINT", "path": "$.edges[2].target",
        "message": "edge `payments--stripe` target `strpe` is not a node",
        "suggestions": ["Did you mean `stripe`?"],
        "patch": [{ "op": "replace", "path": "/edges/2/target", "value": "stripe" }] }
    ],
    "agentHint": "Rendering refused: 2 error(s) [ERR_ACCENT_OVERUSE, ERR_MISSING_ENDPOINT]. …"
  }
}
```

Apply both patches, resubmit, done.

## Interactive output

The HTML file is self-contained (inline SVG, CSS and JS; a CSP forbids network access):

- **Pan & zoom** — wheel/pinch, drag, `+` `-` `0`, fit button.
- **Tracing** — hover or focus a card to highlight everything upstream and downstream.
- **Inspector** — click a card: file, line range, verification state, source excerpt with line
  numbers, docstring, metadata, upstream/downstream lists, *Copy Git Reference* (forge permalink
  pinned to the commit when the remote is GitHub/GitLab/Bitbucket/Codeberg).
- **Theme** — editorial light/dark switch without re-rendering (`t`).
- **Export** — standalone SVG, and PNG at 2–3× via the Canvas API.

## Anti-patterns

- Asking the engine to "just draw" without scanning — you'll invent relationships and line numbers.
- Every service focal "because it's important" — pick the subject of the story.
- Boundaries around single nodes, or one boundary per technology.
- Edge labels that restate the edge type ("sync call") instead of the business verb.
- Fixing `ERR_HIGH_DENSITY` by deleting evidence-backed nodes on the critical path.
