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
	docker compose build autodoc

# Regenerate the committed demonstration book in examples/polyglot-shop.
# The example book ships in this repository, so it must be reproducible on any
# machine: a book records the repository root it was generated from and the
# commit it was pinned to. The fixture is copied to a fixed path inside the
# container, away from this repository's `.git`, so neither leaks into it.
DEMO_IN_CONTAINER = docker compose run --rm --entrypoint sh autodoc -c \
	'cp -R /repo/tests/fixtures/polyglot-shop /tmp/polyglot-shop && \
	 autodoc $(1) /tmp/polyglot-shop --out /repo/examples/polyglot-shop'

demo:
	$(call DEMO_IN_CONTAINER,generate)

demo-check:
	$(call DEMO_IN_CONTAINER,check)

schema:
	docker compose run --rm autodoc schema > schema/diagram-ir.schema.json

clean:
	docker compose down -v
