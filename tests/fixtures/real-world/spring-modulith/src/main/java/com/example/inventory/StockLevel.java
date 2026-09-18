package com.example.inventory;

/** Units on hand for a product. */
public record StockLevel(String sku, int units) {
}
