package com.acme.shop.domain

/** Published once an order is accepted. */
data class OrderPlaced(val orderId: Long, val customerName: String)
