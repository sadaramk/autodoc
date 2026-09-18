package com.acme.shop.domain

import jakarta.persistence.CascadeType
import jakarta.persistence.Column
import jakarta.persistence.Entity
import jakarta.persistence.EnumType
import jakarta.persistence.Enumerated
import jakarta.persistence.Id
import jakarta.persistence.JoinColumn
import jakarta.persistence.ManyToOne
import jakarta.persistence.OneToMany
import jakarta.persistence.Table
import java.math.BigDecimal
import java.time.Instant

/** A customer order. */
@Entity
@Table(name = "orders")
class Order(
    @Id
    val id: Long = 0,

    @Column(name = "customer_name", nullable = false, length = 80)
    val customerName: String = "",

    @Column(name = "total_amount")
    val totalAmount: BigDecimal = BigDecimal.ZERO,

    @Enumerated(EnumType.STRING)
    @Column(nullable = false)
    var status: OrderStatus = OrderStatus.NEW,

    @ManyToOne
    @JoinColumn(name = "customer_id")
    val customer: Customer? = null,

    val placedAt: Instant? = null,

    @OneToMany(mappedBy = "order", cascade = [CascadeType.ALL])
    val items: MutableList<OrderItem> = mutableListOf(),
)

enum class OrderStatus { NEW, PAID, SHIPPED, CANCELLED }
