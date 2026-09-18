package com.acme.orders.domain;

import java.util.List;

import com.acme.orders.api.CreateOrderRequest;
import com.acme.orders.api.OrderDto;
import com.acme.orders.client.InventoryClient;
import org.springframework.stereotype.Service;

@Service
public class OrderService {

    private final InventoryClient inventory;

    public OrderService(InventoryClient inventory) {
        this.inventory = inventory;
    }

    public List<OrderDto> list(OrderStatus status, int limit) {
        return List.of();
    }

    public OrderDto find(Long id) {
        throw new OrderNotFoundException("order not found");
    }

    public boolean inStock(CreateOrderRequest request) {
        return true;
    }

    public OrderDto place(CreateOrderRequest request) {
        if (request.getLines().isEmpty()) {
            throw new PaymentDeclinedException("payment declined");
        }
        return null;
    }

    public void cancel(Long id) {
    }
}
