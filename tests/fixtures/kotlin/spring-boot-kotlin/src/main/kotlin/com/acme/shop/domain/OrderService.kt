package com.acme.shop.domain

import com.acme.shop.audit.AuditEvent
import com.acme.shop.audit.AuditRepository
import org.springframework.context.ApplicationEventPublisher
import org.springframework.stereotype.Service
import java.time.Instant

@Service
class OrderService(
    private val repository: OrderRepository,
    private val audit: AuditRepository,
    private val events: ApplicationEventPublisher,
) {

    fun place(name: String): Order {
        val order = Order(customerName = name)
        order.status = OrderStatus.PAID
        audit.save(AuditEvent(orderId = order.id, action = "placed", at = Instant.now()))
        events.publishEvent(OrderPlaced(order.id, order.customerName))
        return repository.save(order)
    }

    fun byId(id: Long): Order? = repository.findById(id).orElse(null)
}
