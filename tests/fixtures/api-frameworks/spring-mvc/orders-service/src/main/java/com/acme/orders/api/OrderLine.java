package com.acme.orders.api;

import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;

public record OrderLine(@NotBlank String sku, @Min(1) @Max(99) int quantity) {
}
