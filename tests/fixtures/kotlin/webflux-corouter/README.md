# webflux-corouter

A Spring WebFlux service written in Kotlin, routed with the functional coroutine DSL.

Exercises: `coRouter { "/books".nest { GET("", handler::all) … } }` — the path comes from the nested block and
the method reference names the handler; `spring.webflux.base-path: /api`; `request.pathVariable("id")` and
`request.queryParamOrNull("author")` as parameters; `request.awaitBody<NewBook>()` as the request body;
`ServerResponse.status(HttpStatus.CREATED)` as the success status; `ResponseStatusException(NOT_FOUND, …)`
thrown in the handler as a declared error.
