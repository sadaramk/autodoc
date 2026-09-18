package com.acme.shop.audit

import com.acme.shop.domain.OrderPlaced
import org.springframework.context.event.EventListener
import org.springframework.stereotype.Component

@Component
class AuditListener(private val audit: AuditRepository) {

    @EventListener
    fun onOrderPlaced(event: OrderPlaced) {
        audit.save(AuditEvent(orderId = event.orderId, action = "placed"))
    }
}
