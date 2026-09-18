# quarkus-jaxrs

Quarkus REST (Jakarta REST) built with Gradle: `quarkus.rest.path=/api`; class and method `@Path`; `@GET/@POST/@DELETE`;
`@QueryParam` + `@DefaultValue`, `@PathParam`; an un-annotated `@Valid` entity parameter as the body; `Response.status(CREATED)`;
`void` → 204; `NotFoundException`; an `ExceptionMapper` mapping a thrown exception to 422; `@RolesAllowed` / `@Authenticated`;
a MicroProfile `@RegisterRestClient` interface; a `/healthz` probe.
