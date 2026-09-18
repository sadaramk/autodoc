# spring-boot-kotlin

A Spring Boot service written in Kotlin, built with the Gradle Kotlin DSL.

Exercises: `@SpringBootApplication` + `runApplication<T>(*args)` entry point; `server.servlet.context-path`;
`@RestController` + class `@RequestMapping`; `@PathVariable` / `@RequestParam(required, defaultValue)`;
nullable Kotlin types and default arguments as optional parameters; `@Valid @RequestBody` data class with
`@field:` Bean Validation; `@ResponseStatus(HttpStatus.CREATED)`; `ResponseStatusException`; `@PreAuthorize`;
enum parameter and field rules.

On the data side: JPA `@Entity` / `@Table` / `@Column` / `@Enumerated` / `@ManyToOne` + `@JoinColumn` and the
`@OneToMany(mappedBy = …)` inverse side; a `@MappedSuperclass` (`Auditable`) and an `@Embeddable` (`Money`)
flattened into the owning table; a Spring Data `JpaRepository` with a derived finder and a `@Modifying @Query`;
a Spring Data MongoDB `@Document` with `@Field` wire names and its `MongoRepository`; repository access from a
service and from a job; an `OrderStatus` lifecycle; `publishEvent` / `@EventListener` application events;
`@Scheduled` job and `@KafkaListener`.
