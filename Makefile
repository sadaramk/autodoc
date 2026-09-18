.PHONY: test test-rust test-ts e2e lint build demo schema clean

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
demo:
	docker compose run --rm autodoc generate tests/fixtures/polyglot-shop --out examples/polyglot-shop

schema:
	docker compose run --rm autodoc schema > schema/diagram-ir.schema.json

clean:
	docker compose down -v
