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

## Next

**Say what was not read, everywhere.** The scan counts files it recognised but
has no grammar for, and refuses a repository where it recognised nothing. It
does not yet refuse — or loudly qualify — a book built from a small fraction of
a repository. A book covering 5% of a codebase should say so on its first page,
not in a note.

**C# is the biggest hole.** `.cs` is recognised and never parsed, which is why
the example-voting-app book cannot see its worker. Ruby, PHP and Elixir are in
the same position. C# first, because it most often appears in polyglot systems
that nunki otherwise documents well.

**`diff` as a PR comment out of the box.** The command produces the Markdown;
posting it is still the caller's job. A documented workflow — or an action input
— should close that gap, including updating one comment instead of appending a
new one on every push.

**Publish the action to the GitHub Marketplace**, once the name question is
settled.

## Later

**Large repositories.** etcd and MinIO scan, but the whole tree is read every
time. Caching parsed facts against content hashes would make `check` cheap
enough to run on every push of a large repository.

**Tracing through runtime indirection.** Request flows stop where a dependency
is chosen at runtime — a DI container binding one of two implementations of the
same interface, a CQRS handler resolved from a registry. A cross-unit expansion
was built and reverted because it changed nothing on five real repositories; the
blocker is dispatch, not reach. This needs a different approach, not a bigger
search.

**Authored content that survives better.** `authored.json` is created once and
never overwritten, which is right, but there is no way to say "this section is
now wrong" when the code it describes moves.

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

1. **`DiagramIR` is stable.** The schema is versioned and the TypeScript mirror
   is a contract test, but fields still move between minor versions.
2. **The CLI surface is stable.** Subcommands and flags stop changing shape.
3. **No known case where the book states something the code does not support.**
   Every such case found so far became a fix and a regression fixture; the bar
   for 1.0 is that the list is empty, not short. The findings are in
   [HEURISTICS.md](HEURISTICS.md).
