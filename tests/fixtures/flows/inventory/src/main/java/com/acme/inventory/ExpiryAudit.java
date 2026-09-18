package com.acme.inventory;

import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.stereotype.Component;
import org.springframework.transaction.event.TransactionalEventListener;

@Component
public class ExpiryAudit {

    private final JdbcTemplate jdbc;

    public ExpiryAudit(JdbcTemplate jdbc) {
        this.jdbc = jdbc;
    }

    /** Takes the event type from the annotation, not the parameter. */
    @TransactionalEventListener(classes = StockExpired.class)
    public void onExpiry(Object raw) {
        jdbc.update("INSERT INTO stock_audit (sku) VALUES ('expired')");
    }
}
