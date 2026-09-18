package com.acme.catalog

import jakarta.validation.constraints.NotBlank
import jakarta.validation.constraints.Positive
import org.springframework.http.HttpStatus
import org.springframework.stereotype.Component
import org.springframework.web.reactive.function.server.ServerRequest
import org.springframework.web.reactive.function.server.ServerResponse
import org.springframework.web.reactive.function.server.awaitBody
import org.springframework.web.reactive.function.server.bodyValueAndAwait
import org.springframework.web.reactive.function.server.buildAndAwait
import org.springframework.web.server.ResponseStatusException

/** Handles the catalog's book routes. */
@Component
class BookHandler(private val books: BookStore) {

    suspend fun all(request: ServerRequest): ServerResponse {
        val author = request.queryParamOrNull("author")
        return ServerResponse.ok().bodyValueAndAwait(books.all(author))
    }

    suspend fun byId(request: ServerRequest): ServerResponse {
        val id = request.pathVariable("id")
        val book = books.byId(id) ?: throw ResponseStatusException(HttpStatus.NOT_FOUND, "no such book")
        return ServerResponse.ok().bodyValueAndAwait(book)
    }

    suspend fun create(request: ServerRequest): ServerResponse {
        val body = request.awaitBody<NewBook>()
        val saved = books.add(body)
        return ServerResponse.status(HttpStatus.CREATED).bodyValueAndAwait(saved)
    }
}

/** A book as the catalog accepts it. */
data class NewBook(
    @field:NotBlank val title: String,
    @field:NotBlank val author: String,
    @field:Positive val pages: Int = 1,
)
