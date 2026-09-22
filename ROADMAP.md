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
| | Authored prose can be pinned to evidence; `check` reports it when it rots |

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
amount of work completes — only elapsed time without a violation does. The
machinery that would notice a violation is in place, so the clock is running
underneath everything below.

| | | |
|---|---|---|
| 1 | [#20](https://github.com/sadaramk/nunki/issues/20) | Keep a history of how the architecture changed, release by release. `diff` already answers it between any two tags; every answer is currently thrown away. |
| 2 | [#21](https://github.com/sadaramk/nunki/issues/21) | Document a system that spans several repositories. Larger, and constrained by the same promise: a citation into a sibling repository has to be verified, not trusted. |
| — | [#8](https://github.com/sadaramk/nunki/issues/8) · [#9](https://github.com/sadaramk/nunki/issues/9) | Running. Nothing to do but not break them. |

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

- **Following dispatch that happens at runtime.** Twice investigated, twice
  abandoned. A cross-unit expansion was built and reverted during 0.2 for
  changing nothing on five real repositories; instrumenting the code path
  later showed why, on five more — the function that resolves a port to its
  adapter runs zero times on eShopOnWeb, CleanArchitecture, nunki itself and
  the Spring Cloud fixture, and the multi-implementation case it is blamed for
  never fired once. Whatever limits a request flow is upstream of interface
  dispatch. If that changes, the approach is to read the dependency-injection
  registration — `AddScoped<IOrderService, OrderService>` is an ordinary
  citable line, and it names decorators a single-implementation guess gets
  wrong — not to search harder.
  ([#6](https://github.com/sadaramk/nunki/issues/6))

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
