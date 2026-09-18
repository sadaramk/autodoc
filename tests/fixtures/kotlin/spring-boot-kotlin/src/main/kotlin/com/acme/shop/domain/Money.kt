package com.acme.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.Embeddable
import java.math.BigDecimal

/** An amount and the currency it is in, stored on the owning row. */
@Embeddable
class Money(
    @Column(name = "amount", precision = 12, scale = 2)
    val amount: BigDecimal = BigDecimal.ZERO,

    @Column(name = "currency", length = 3, nullable = false)
    val currency: String = "EUR",
)
