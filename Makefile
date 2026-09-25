.PHONY: test test-rust test-ts e2e lint build demo demo-check schema clean

# Everything runs in Docker (see docker-compose.yml).
test: test-rust test-ts e2e

test-rust:
	docker compose run --rm test

test-ts:
	docker compose run --rm test-ts

e2e:
	docker compose build e2e
	docker compose run --rm e2e

lint:
	docker compose run --rm lint

build:
	docker compose build nunki

# Regenerate the committed demonstration book in examples/polyglot-shop.
# The example book ships in this repository, so it must be reproducible on any
# machine: a book records the repository root it was generated from and the
# commit it was pinned to. The fixture is copied to a fixed path inside the
# container, away from this repository's `.git`, so neither leaks into it.
#
# Built in the `test` service, which bind-mounts this working tree, rather than
# in an image built from it. An image stage that copies the source and then
# compiles can hand cargo normalised mtimes against a cached target directory;
# cargo concludes everything is fresh, does nothing, and the demo regenerates
# the example with the *previous* binary. `demo-check` then passes, because it
# checks the book against the same stale binary that wrote it — which is the
# drift these targets exist to catch, arriving silently through the mechanism
# meant to prevent it (#62). A bind mount has real mtimes and no copy step, so
# cargo's own freshness check is the one doing the work.
DEMO_BIN = target/release/nunki
DEMO_FIXTURE = /tmp/polyglot-shop

# Belt and braces: cargo guarantees this after a successful build, and it is
# the exact invariant that broke, so it is worth asserting rather than assuming.
DEMO_IN_CONTAINER = docker compose run --rm test sh -c \
	'set -e; \
	 cargo build --release --locked -p nunki-cli; \
	 newer=$$(find crates -name "*.rs" -newer $(DEMO_BIN) -print -quit); \
	 [ -z "$$newer" ] || { echo "refusing: $(DEMO_BIN) is older than $$newer" >&2; exit 1; }; \
	 rm -rf $(DEMO_FIXTURE); \
	 cp -R /workspace/tests/fixtures/polyglot-shop $(DEMO_FIXTURE); \
	 $(DEMO_BIN) $(1) $(DEMO_FIXTURE) --out /workspace/examples/polyglot-shop'

demo:
	$(call DEMO_IN_CONTAINER,generate)

demo-check:
	$(call DEMO_IN_CONTAINER,check)

schema:
	docker compose run --rm nunki schema > schema/diagram-ir.schema.json

clean:
	docker compose down -v
