package com.acme.shop.jobs

import com.acme.shop.domain.OrderRepository
import org.springframework.kafka.annotation.KafkaListener
import org.springframework.scheduling.annotation.Scheduled
import org.springframework.stereotype.Component
import java.time.Instant

@Component
class StaleOrders(private val repository: OrderRepository) {

    /** Cancels orders left unpaid overnight. */
    @Scheduled(cron = "0 0 3 * * *")
    fun cancelStale() {
        repository.cancelStale(Instant.now())
    }

    @KafkaListener(topics = ["payments.settled"])
    fun onSettled(payload: String) {
        repository.findByStatus(com.acme.shop.domain.OrderStatus.NEW)
    }
}
