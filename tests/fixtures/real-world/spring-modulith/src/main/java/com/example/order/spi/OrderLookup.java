package com.example.order.spi;

/** Read-only view of orders for other modules. */
public interface OrderLookup {

    boolean exists(String orderId);
}
