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
