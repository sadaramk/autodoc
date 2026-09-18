package com.example.shipping;

import com.example.inventory.StockLevel;
import com.example.order.internal.OrderRepository;
import com.example.order.spi.OrderLookup;
import org.springframework.stereotype.Service;

/** Ships orders once stock is confirmed. */
@Service
public class ShippingService {

    private final OrderLookup orders;
    private final OrderRepository repository;

    public ShippingService(OrderLookup orders, OrderRepository repository) {
        this.orders = orders;
        this.repository = repository;
    }

    public boolean canShip(String orderId, StockLevel stock) {
        return orders.exists(orderId) && stock.units() > 0;
    }
}
