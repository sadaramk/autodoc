# Roadmap

What this project is for, what is next, and — just as usefully — what it will
never do. Dates are deliberately absent; the ordering is the commitment.

The measure for every item below is the same one the tool applies to itself: a
claim that cannot be traced to the code is a bug, not a feature. Anything that
would make nunki confident about something it has not read does not ship,
however useful it sounds.

## Shipped

| | |
|---|---|
| **0.1** | The architecture book: containers, relationships, C4 depths, typed `DiagramIR`, deterministic layout, interactive HTML |
| **0.2** | Behaviour: API contracts, request-flow sequences, data model and lifecycles, functional spec and BRD scaffold, capability and domain splits, runtime topology |
| | Evidence pinned to a commit and verified against it; `check` fails CI on drift |
| | MCP server on stdio, for agents |
| | Prebuilt binaries for five targets, an installer, and a GitHub Action |
| | `diff` — what two revisions disagree about, as a PR comment |
| | `export --format drawio` — the diagram, with its evidence, in a tool you can edit |

Languages read today: Rust, TypeScript/TSX, Go, Python, Java, Kotlin.

## Tracked work

Every item below is an issue, so progress is visible without reading this file.
**[Board](https://github.com/users/sadaramk/projects/3)** ·
[Open issues](https://github.com/sadaramk/nunki/issues) ·
[0.4.0](https://github.com/sadaramk/nunki/milestone/1) ·
[1.0](https://github.com/sadaramk/nunki/milestone/2)

On the board, *Blocked* means waiting on a decision or on someone outside the
repository — not on effort.

### 0.4.0

| | |
|---|---|
| [#1](https://github.com/sadaramk/nunki/issues/1) | Qualify or refuse a book built from a small fraction of a repository |
| [#2](https://github.com/sadaramk/nunki/issues/2) | Read C# — `.cs` is recognised and never parsed |
| [#3](https://github.com/sadaramk/nunki/issues/3) | Post `diff` as a PR comment out of the box |
| [#10](https://github.com/sadaramk/nunki/issues/10) | Publish the action to the GitHub Marketplace |

### Needs a decision first

| | |
|---|---|
| [#4](https://github.com/sadaramk/nunki/issues/4) | Whether the containers diagram keeps datastore edges |

### Later, unscheduled

| | |
|---|---|
| [#5](https://github.com/sadaramk/nunki/issues/5) | Cache parsed facts so `check` is cheap on large repositories |
| [#6](https://github.com/sadaramk/nunki/issues/6) | Trace through runtime indirection (DI containers, CQRS registries) |
| [#7](https://github.com/sadaramk/nunki/issues/7) | Detect when authored content has gone stale |

## Not doing

These are not backlog items. They are decisions, and the reasoning matters more
than the list.

- **Prose written by a language model.** The entire value here is that a claim
  can be traced to a line of code. Generated narrative cannot be, and mixing the
  two makes the verifiable parts untrustworthy by association.
- **A hosted service.** The book is files you commit next to the code. A server
  in the middle adds an outage, an account and a bill to something that works
  offline.
- **A diagram editor.** draw.io is a good editor and this is not one. Export is
  one way on purpose: the source stays authoritative about the architecture.
- **An architecture rule engine.** "No module may import X" is a linter's job,
  and there are good ones. Documenting what is true is a different problem from
  enforcing what should be.
- **Runtime or OpenTelemetry tracing.** A different kind of evidence, needing a
  running system, a collector and a retention policy. It would also make the
  output non-reproducible, which `check` depends on.
- **A developer portal.** Backstage exists.

## What 1.0 means

Not a feature count. Three things have to be true:

1. **[`DiagramIR` is stable](https://github.com/sadaramk/nunki/issues/8).** The
   schema is versioned and the TypeScript mirror is a contract test, but fields
   still move between minor versions.
2. **[The CLI surface is stable](https://github.com/sadaramk/nunki/issues/9).**
   Subcommands and flags stop changing shape.
3. **No known case where the book states something the code does not support.**
   Every such case found so far became a fix and a regression fixture; the bar
   for 1.0 is that the list is empty, not short. The findings are in
   [HEURISTICS.md](HEURISTICS.md).
