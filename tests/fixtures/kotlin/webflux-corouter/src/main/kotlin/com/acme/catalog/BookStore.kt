package com.acme.catalog

import org.springframework.stereotype.Component

@Component
class BookStore {
    private val rows = mutableMapOf<String, NewBook>()

    fun all(author: String?): List<NewBook> = rows.values.filter { author == null || it.author == author }

    fun byId(id: String): NewBook? = rows[id]

    fun add(book: NewBook): NewBook {
        rows[book.title] = book
        return book
    }
}
