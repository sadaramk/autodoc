package com.acme.inventory;

public class StockExpiredBuilder {

    private String sku;

    public StockExpiredBuilder sku(String sku) {
        this.sku = sku;
        return this;
    }

    public StockExpired build() {
        return new StockExpired(sku);
    }
}
