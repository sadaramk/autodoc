# API framework fixtures

One tiny, syntactically valid app per framework, used by `crates/analyzer/tests/api_contracts.rs` to check that
API-contract extraction resolves full paths (router prefixes, mounts, scopes, global prefixes), parameters,
request/response models with validation rules, error statuses, auth requirements and excluded probes.

| Fixture | Exercises |
|---------|-----------|
| `express/` | `Router()` nested three deep via `app.use("/api", api)` + `api.use("/users", usersRouter())` (factory); `:id` params; `requireRole("admin")` middleware; zod body with `.email()` / enum / optional; `Request<Params, ResBody, ReqBody, Query>` generics; thrown `NotFoundError`; `/healthz` probe. |
| `fastify/` | `app.register(plugin, { prefix: "/v1" })`; route generics `{ Body; Reply; Querystring }`; `preHandler: [authenticate]`; `reply.code(201)`. |
| `nestjs/` | `setGlobalPrefix("api")`; `@Controller("orders")`; `@Get(":id")` with `@Param`/`@Query`; `@Body()` DTO with class-validator decorators; `@UseGuards` / `@Roles`; `@HttpCode(204)`; `NotFoundException`. |
| `hono/` | `app.route("/books", books)`; `zValidator("json", schema)`; `c.json(body, 201)`; `c.req.param` / `c.req.query`. |
| `fastapi/` | `settings.API_PREFIX` constant from a settings class; `APIRouter(prefix=…, dependencies=[Depends(get_current_admin)])`; nested `include_router(prefix=…)`; `Annotated[…, Query(ge=…)]`; `Path` rules; Pydantic models with inheritance, `alias`, `Field(...)`, enums; `HTTPException`; `/docs`-style probe excluded. |
| `flask/` | `Blueprint(url_prefix=…)` + `register_blueprint(url_prefix=…)`; `<int:post_id>`; `methods=[…]`; `abort(404)`; `jsonify(…), 201`; `@login_required`; `request.args.get`. |
| `go-chi/` | chi `Route` closures, `Mount("/admin", adminRoutes())`, `With(requireAuth)`; `chi.URLParam`, `URL.Query().Get`; `writeJSON(w, http.StatusCreated, …)`; `validate` tags. |
| `gin/` | `v1 := r.Group("/api/v1")`, nested groups, `Use(AuthRequired())`; `ShouldBindJSON`; `binding` tags; `c.JSON(http.StatusNotFound, gin.H{"error": …})`; `c.Param` / `c.DefaultQuery`. |
| `axum/` | `nest("/api", api_routes())`, `merge`, `route_layer(from_fn(require_auth))`; `Path` / `Query<T>` / `Json<T>` extractors; `(StatusCode::CREATED, Json<T>)`; serde `rename_all` + `rename` + `default`; validator `length` / `range` / `email`. |
| `actix/` | `web::scope("/api")` with nested scopes, `.service(handler)` for `#[get]` macros, `.route("/x", web::post().to(h))`, `HttpResponse::Created()` / `NotFound()`; `web::Json` / `web::Path`. |
| `spring-mvc/` | Maven reactor of two Spring Boot modules; `server.servlet.context-path`; class + method `@RequestMapping` / `@GetMapping`…; `@RequestParam(required, defaultValue)`, `@PathVariable("id")`, `@RequestHeader`; `@Valid @RequestBody` with Bean Validation, `@JsonProperty`, nested records and an enum; `@ResponseStatus`; `ResponseStatusException`; `@ResponseStatus` exception thrown in a service; `@RestControllerAdvice`; `@PreAuthorize`; `requestMatchers(…).authenticated()`; `@FeignClient` → the other module, with response drift. |
| `spring-webflux-functional/` | `RouterFunctions.route().path(…, builder -> builder.GET(…))` with `handler::method`; `bodyToMono(T.class)` / `body(…, T.class)`; `ServerResponse.status(CREATED)` / `notFound()`; probe. |
| `quarkus-jaxrs/` | Gradle; `quarkus.rest.path`; JAX-RS `@Path` / `@GET`…; `@QueryParam` + `@DefaultValue`; entity parameter body; `Response.status(CREATED)`; `void` → 204; `NotFoundException`; `ExceptionMapper`; `@RolesAllowed` / `@Authenticated`; `@RegisterRestClient`. |
| `micronaut/` | `micronaut.server.context-path`; `@Controller` / `@Get`…; path binding by name; `@QueryValue`; `@Body`; `@Status`; `HttpResponse.noContent()` / `notFound()`; `@Secured`. |
| `ktor/` | Kotlin: `embeddedServer(Netty) { }` entry; `routing { route("/books") { get / post } }` nested prefixes; `authenticate("jwt") { }`; `call.receive<T>()` body; `call.respond(HttpStatusCode.Created, …)`; `call.parameters["id"]` and `queryParameters["q"]`; `/healthz` probe excluded. |
