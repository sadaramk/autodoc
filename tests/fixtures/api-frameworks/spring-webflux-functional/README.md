# spring-webflux-functional

Spring WebFlux functional routing: `RouterFunctions.route().path("/api/books", builder -> builder.GET(…).POST(…))`
with `handler::method` references, `bodyToMono(NewBook.class)`, `body(…, Book.class)`, `ServerResponse.status(CREATED)`
and `notFound()`; a `route(GET("/healthz"), …)` probe.
