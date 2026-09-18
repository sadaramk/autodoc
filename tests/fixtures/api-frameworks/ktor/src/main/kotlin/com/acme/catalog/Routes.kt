package com.acme.catalog

import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.application.call
import io.ktor.server.auth.authenticate
import io.ktor.server.request.receive
import io.ktor.server.response.respond
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.route
import io.ktor.server.routing.routing
import kotlinx.serialization.Serializable

@Serializable
data class Book(val id: String, val title: String, val pages: Int? = null)

@Serializable
data class NewBook(val title: String, val author: String, val pages: Int = 0)

fun Application.catalogModule() {
    routing {
        route("/books") {
            get {
                val q = call.request.queryParameters["q"]
                call.respond(listOf<Book>())
            }

            get("/{id}") {
                val id = call.parameters["id"]
                call.respond(Book(id ?: "", "Dune"))
            }

            authenticate("jwt") {
                post {
                    val body = call.receive<NewBook>()
                    call.respond(HttpStatusCode.Created, Book("1", body.title))
                }
            }
        }

        get("/healthz") {
            call.respond(HttpStatusCode.OK)
        }
    }
}
