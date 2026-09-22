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
| **0.4** | A book states how much of the source it read, and refuses below a tenth |
| | C# and ASP.NET Core: attribute routes, EF Core entities, `.csproj` modules |
| | `comment: true` — the action keeps one diff comment per pull request |
| | On the Marketplace as [Nunki Architecture Docs](https://github.com/marketplace/actions/nunki-architecture-docs) |

Languages read today: Rust, TypeScript/TSX, Go, Python, Java, Kotlin, C#.

## Tracked work

Every item below is an issue, so progress is visible without reading this file.
**[Board](https://github.com/users/sadaramk/projects/3)** ·
[Open issues](https://github.com/sadaramk/nunki/issues) ·
[0.4.0](https://github.com/sadaramk/nunki/milestone/1) ·
[1.0](https://github.com/sadaramk/nunki/milestone/2)

On the board, *Blocked* means waiting on a decision or on someone outside the
repository — not on effort.

### 0.4.0 — shipped

| | |
|---|---|
| [#1](https://github.com/sadaramk/nunki/issues/1) | Qualify or refuse a book built from a small fraction of a repository |
| [#2](https://github.com/sadaramk/nunki/issues/2) | Read C# |
| [#3](https://github.com/sadaramk/nunki/issues/3) | Post `diff` as a PR comment out of the box |
| [#4](https://github.com/sadaramk/nunki/issues/4) | Containers keeps its datastore edges; two boxes are joined by one connector |
| [#10](https://github.com/sadaramk/nunki/issues/10) | Publish the action to the GitHub Marketplace |

### What is next, in order

The ordering is the commitment, and it is not by size. **[#8](https://github.com/sadaramk/nunki/issues/8)
and [#9](https://github.com/sadaramk/nunki/issues/9) are clocks rather than
tasks**: both promise a surface *unchanged for a full minor cycle*, which no
amount of work completes — only elapsed time without a violation does. Nothing
can elapse until a violation would be noticed, so the machinery that notices
comes first and the clock then runs underneath everything else.

| | | |
|---|---|---|
| 1 | [#8](https://github.com/sadaramk/nunki/issues/8) · [#9](https://github.com/sadaramk/nunki/issues/9) | Publish the schema with each release; tell a breaking schema change from an additive one; snapshot the CLI surface; write the deprecation path. Starts the clock. |
| 2 | [#7](https://github.com/sadaramk/nunki/issues/7) | Detect when authored prose has gone stale — the one place the project still publishes a claim it has not checked. |
| 3 | [#6](https://github.com/sadaramk/nunki/issues/6) | Trace through runtime indirection. A spike with an exit, not a feature: one attempt already changed nothing on five real repositories. |

## Not doing

These are not backlog items. They are decisions, and the reasoning matters more
than the list.

- **A parsed-fact cache.** Measured before building, on five large
  repositories: a complete, fully verified book takes 0.3 s on etcd and 9.4 s
  on kubernetes — 8,167 files and 1.8 million lines, where `git clone` alone
  takes minutes. The premise was right that `check` re-parses everything every
  run; the cost was not. A cache would buy a few seconds and introduce the one
  failure this project exists to prevent, a stale entry producing a book that
  looks correct. If it ever does become painful the answer is to parallelise
  the behaviour phase — parsing already is — which needs the same restructure
  without the staleness. ([#5](https://github.com/sadaramk/nunki/issues/5))

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
