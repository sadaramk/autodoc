package com.acme.books;

import org.springframework.http.HttpStatus;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.server.ServerRequest;
import org.springframework.web.reactive.function.server.ServerResponse;
import reactor.core.publisher.Flux;
import reactor.core.publisher.Mono;

@Component
public class BookHandler {

    public Mono<ServerResponse> list(ServerRequest request) {
        return ServerResponse.ok().body(Flux.empty(), Book.class);
    }

    public Mono<ServerResponse> get(ServerRequest request) {
        String isbn = request.pathVariable("isbn");
        return find(isbn)
                .flatMap(book -> ServerResponse.ok().body(Mono.just(book), Book.class))
                .switchIfEmpty(ServerResponse.notFound().build());
    }

    public Mono<ServerResponse> create(ServerRequest request) {
        return request.bodyToMono(NewBook.class)
                .flatMap(book -> ServerResponse.status(HttpStatus.CREATED).bodyValue(book));
    }

    private Mono<Book> find(String isbn) {
        return Mono.empty();
    }
}
