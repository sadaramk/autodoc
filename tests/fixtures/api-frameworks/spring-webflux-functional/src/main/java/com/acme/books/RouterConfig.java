package com.acme.books;

import static org.springframework.web.reactive.function.server.RequestPredicates.GET;
import static org.springframework.web.reactive.function.server.RouterFunctions.route;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.web.reactive.function.server.RouterFunction;
import org.springframework.web.reactive.function.server.RouterFunctions;
import org.springframework.web.reactive.function.server.ServerResponse;

@Configuration
public class RouterConfig {

    @Bean
    RouterFunction<ServerResponse> bookRoutes(BookHandler handler) {
        return RouterFunctions.route()
                .path("/api/books", builder -> builder
                        .GET("", handler::list)
                        .GET("/{isbn}", handler::get)
                        .POST("", handler::create))
                .build();
    }

    @Bean
    RouterFunction<ServerResponse> probes() {
        return route(GET("/healthz"), request -> ServerResponse.ok().build());
    }
}
