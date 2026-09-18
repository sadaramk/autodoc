# spring-mvc

Two Spring Boot modules in a Maven reactor. `orders-service` (context path `/api`) publishes `/orders` with
`@RequestParam` / `@PathVariable` / `@RequestHeader`, a validated `@RequestBody` (Bean Validation, Jackson
`@JsonProperty`, nested records, enum), `@ResponseStatus`, `ResponseStatusException`, an exception annotated
`@ResponseStatus` thrown one call deep, a `@RestControllerAdvice` mapping, `@PreAuthorize`, and security matchers.
Its `@FeignClient(name = "inventory-service")` calls `inventory-service`, whose response lacks `expiresAt` (drift).
