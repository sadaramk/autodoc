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
# The example book ships in this repository, so it must be reproducible: the
# container sees the fixture at a fixed path and without this repository's
# `.git`, so the book doesn't move with our commits or anyone's checkout path.
demo:
	docker compose run --rm autodoc generate tests/fixtures/polyglot-shop --out examples/polyglot-shop

demo-check:
	docker compose run --rm autodoc check tests/fixtures/polyglot-shop --out examples/polyglot-shop

schema:
	docker compose run --rm autodoc schema > schema/diagram-ir.schema.json

clean:
	docker compose down -v
