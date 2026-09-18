package com.acme.quotes.web

import com.acme.pricing.PriceCalculator
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.RequestParam
import org.springframework.web.bind.annotation.RestController
import java.math.BigDecimal

/** Quotes a price for a basket. */
@RestController
class QuoteController(private val prices: PriceCalculator) {

    @GetMapping("/quote")
    fun quote(@RequestParam sku: String, @RequestParam(defaultValue = "1") quantity: Int): BigDecimal =
        prices.total(sku, quantity)
}
