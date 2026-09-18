package com.acme.catalog

import org.springframework.context.annotation.Bean
import org.springframework.context.annotation.Configuration
import org.springframework.web.reactive.function.server.coRouter

/** Every route the catalog serves, and the handler behind it. */
@Configuration
class CatalogRoutes {

    @Bean
    fun routes(handler: BookHandler) = coRouter {
        "/books".nest {
            GET("", handler::all)
            GET("/{id}", handler::byId)
            POST("", handler::create)
        }
    }
}
