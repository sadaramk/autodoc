# kotlin-modules

A Gradle multi-module Kotlin build: a Spring Boot service and the shared library it depends on.

Exercises: `include(":app", ":pricing")` — the settings root is not a unit; `app` is an HTTP service
(`@SpringBootApplication`, `@RestController`); `pricing` stays a **library** even though it holds a `@Service`,
because framework code alone does not make a module deployable; the `implementation(project(":pricing"))`
edge is cited at the `com.acme.pricing` import that actually uses it, not at the build file; a `@RequestParam`
with no default is required, one with `defaultValue` is not.

Also: a second service (`catalog`, application name `catalog-service`) that `app` calls two ways — a
`@FeignClient(name = "catalog-service")` interface and a Ktor `httpClient.get("http://catalog-service/…")` —
so a Kotlin service produces cross-service calls. The Feign interface returns the caller's own `CatalogItem`,
which carries a `stock` field the catalog service never returns: that is reported as response drift.
