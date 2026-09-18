package com.example.order.internal;

import com.example.order.Order;
import org.springframework.data.jpa.repository.JpaRepository;

/** Persistence detail of the order module. */
public interface OrderRepository extends JpaRepository<Order, String> {
}
