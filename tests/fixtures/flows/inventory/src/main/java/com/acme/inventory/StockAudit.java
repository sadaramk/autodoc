package com.acme.inventory;

import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.stereotype.Component;
import org.springframework.transaction.event.TransactionalEventListener;

@Component
public class StockAudit {

    private final JdbcTemplate jdbc;

    public StockAudit(JdbcTemplate jdbc) {
        this.jdbc = jdbc;
    }

    @TransactionalEventListener
    public void on(StockReserved event) {
        jdbc.update("INSERT INTO stock_audit (sku) VALUES (?)", event.orderId());
    }
}
