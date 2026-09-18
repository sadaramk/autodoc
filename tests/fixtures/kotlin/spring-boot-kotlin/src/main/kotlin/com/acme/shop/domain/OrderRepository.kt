package com.acme.shop.domain

import org.springframework.data.jpa.repository.JpaRepository
import org.springframework.data.jpa.repository.Modifying
import org.springframework.data.jpa.repository.Query

interface OrderRepository : JpaRepository<Order, Long> {
    fun findByStatus(status: OrderStatus): List<Order>

    @Modifying
    @Query("update Order o set o.status = 'CANCELLED' where o.placedAt < :cutoff")
    fun cancelStale(cutoff: java.time.Instant): Int
}
