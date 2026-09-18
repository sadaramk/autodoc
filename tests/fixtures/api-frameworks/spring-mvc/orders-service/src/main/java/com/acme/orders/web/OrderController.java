package com.acme.orders.web;

import java.util.List;

import com.acme.orders.api.CreateOrderRequest;
import com.acme.orders.api.OrderDto;
import com.acme.orders.domain.OrderService;
import com.acme.orders.domain.OrderStatus;
import jakarta.validation.Valid;
import jakarta.validation.constraints.Max;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.*;
import org.springframework.web.server.ResponseStatusException;

/** Order placement and lookup. */
@RestController
@RequestMapping("/orders")
public class OrderController {

    private final OrderService orders;

    public OrderController(OrderService orders) {
        this.orders = orders;
    }

    /** Lists the caller's orders, newest first. */
    @GetMapping
    public List<OrderDto> list(@RequestParam(name = "status", required = false) OrderStatus status,
                               @RequestParam(defaultValue = "20") @Max(100) int limit) {
        return orders.list(status, limit);
    }

    @GetMapping("/{id}")
    public ResponseEntity<OrderDto> get(@PathVariable("id") Long orderId) {
        return ResponseEntity.ok(orders.find(orderId));
    }

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    public OrderDto create(@Valid @RequestBody CreateOrderRequest request,
                           @RequestHeader("Idempotency-Key") String idempotencyKey) {
        if (!orders.inStock(request)) {
            throw new ResponseStatusException(HttpStatus.CONFLICT, "out of stock");
        }
        return orders.place(request);
    }

    @DeleteMapping("/{id}")
    @PreAuthorize("hasRole('ADMIN')")
    public ResponseEntity<Void> cancel(@PathVariable Long id) {
        orders.cancel(id);
        return ResponseEntity.noContent().build();
    }
}
