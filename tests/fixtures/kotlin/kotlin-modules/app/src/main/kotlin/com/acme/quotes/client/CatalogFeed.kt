package com.acme.quotes.client

import io.ktor.client.HttpClient
import io.ktor.client.call.body
import io.ktor.client.request.get

/** Ktor client call to the same service, for the bulk feed. */
class CatalogFeed(private val httpClient: HttpClient) {

    suspend fun all(): List<CatalogItem> = httpClient.get("http://catalog-service/catalog/all").body()
}

data class CatalogItem(val sku: String, val priceCents: Long, val stock: Int)
