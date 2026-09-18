package com.acme.quotes.client

import org.springframework.cloud.openfeign.FeignClient
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable

/** Reads catalog prices over HTTP. Its own view of an item carries `stock`,
 * which the catalog service does not return. */
@FeignClient(name = "catalog-service")
interface CatalogClient {

    @GetMapping("/catalog/{sku}")
    fun item(@PathVariable sku: String): CatalogItem
}
