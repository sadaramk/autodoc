# micronaut

Micronaut HTTP server: `micronaut.server.context-path=/v1`; `@Controller("/pets")`; `@Get/@Post/@Delete` with URI
templates; an un-annotated `Long id` bound from the path; `@QueryValue(defaultValue)`, `@Nullable`; `@Body @Valid` record;
`@Status(CREATED)`; `HttpResponse.notFound()` / `noContent()`; `@Secured(IS_AUTHENTICATED)` with a method-level role override.
