package com.acme.orders.api;

import java.util.List;

import com.acme.orders.domain.OrderStatus;
import com.fasterxml.jackson.annotation.JsonProperty;

public record OrderDto(Long id, OrderStatus status, @JsonProperty("total_cents") long totalCents, List<OrderLine> lines) {
}
