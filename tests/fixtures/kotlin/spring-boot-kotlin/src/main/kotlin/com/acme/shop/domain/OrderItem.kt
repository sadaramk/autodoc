package com.acme.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.Embedded
import jakarta.persistence.Entity
import jakarta.persistence.GeneratedValue
import jakarta.persistence.Id
import jakarta.persistence.JoinColumn
import jakarta.persistence.ManyToOne
import jakarta.persistence.Table
import jakarta.validation.constraints.Positive

/** A line on an order. */
@Entity
@Table(name = "order_items")
class OrderItem(
    @Id
    @GeneratedValue
    val id: Long = 0,

    @ManyToOne
    @JoinColumn(name = "order_id", nullable = false)
    val order: Order? = null,

    @Column(name = "sku", nullable = false, length = 32)
    val sku: String = "",

    @field:Positive
    val quantity: Int = 1,

    @Embedded
    val price: Money = Money(),
) : Auditable()
