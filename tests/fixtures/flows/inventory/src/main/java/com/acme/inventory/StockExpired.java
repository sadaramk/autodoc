package com.acme.inventory;

/** Published when a reservation is released. */
public record StockExpired(String sku) {

    public static StockExpiredBuilder builder() {
        return new StockExpiredBuilder();
    }
}
