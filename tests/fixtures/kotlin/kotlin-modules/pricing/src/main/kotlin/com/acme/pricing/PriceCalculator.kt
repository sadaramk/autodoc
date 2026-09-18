package com.acme.pricing

import org.springframework.stereotype.Service
import java.math.BigDecimal

/** Prices a basket line. Shared by every service that quotes. */
@Service
class PriceCalculator {

    fun total(sku: String, quantity: Int): BigDecimal =
        BigDecimal(quantity).multiply(unitPrice(sku))

    private fun unitPrice(sku: String): BigDecimal =
        if (sku.startsWith("BULK")) BigDecimal("4.50") else BigDecimal("9.90")
}
