package com.acme.catalog

import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.RestController

/** What a product costs and whether it can be sold. */
data class CatalogItem(val sku: String, val priceCents: Long)

@RestController
@RequestMapping("/catalog")
class CatalogController {

    @GetMapping("/{sku}")
    fun item(@PathVariable sku: String): CatalogItem = CatalogItem(sku, 1000)
}
