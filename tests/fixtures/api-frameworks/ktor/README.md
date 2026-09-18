# ktor

A Ktor server using the routing DSL.

Exercises: `embeddedServer(...).start(...)` entry point; `routing { route("/books") { … } }` prefixes;
verb lambdas (`get`, `get("/{id}")`, `post`); `call.parameters["id"]` as a path parameter and
`call.request.queryParameters["q"]` as a query parameter; `call.receive<NewBook>()` request body;
`call.respond(HttpStatusCode.Created, …)`; `authenticate("jwt") { … }` wrapping a route; `/healthz` probe excluded.
