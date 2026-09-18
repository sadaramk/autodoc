package com.acme.inventory;

import org.springframework.context.ApplicationEventPublisher;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.kafka.annotation.KafkaListener;
import org.springframework.stereotype.Component;

/** Reserves stock for every new order. */
@Component
public class OrderListener {

    private final JdbcTemplate jdbc;
    private final ApplicationEventPublisher events;

    public OrderListener(JdbcTemplate jdbc, ApplicationEventPublisher events) {
        this.jdbc = jdbc;
        this.events = events;
    }

    @KafkaListener(topics = "order.created", groupId = "inventory")
    public void onOrder(String orderId) {
        jdbc.update("UPDATE stock SET reserved = reserved + 1 WHERE sku = ?", orderId);
        events.publishEvent(new StockReserved(orderId));
    }
}
