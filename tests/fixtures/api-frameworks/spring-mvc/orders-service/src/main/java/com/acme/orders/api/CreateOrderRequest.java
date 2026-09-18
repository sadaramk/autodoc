package com.acme.orders.api;

import java.util.List;

import com.fasterxml.jackson.annotation.JsonProperty;
import jakarta.validation.Valid;
import jakarta.validation.constraints.Email;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotEmpty;
import jakarta.validation.constraints.Pattern;
import jakarta.validation.constraints.Size;

public class CreateOrderRequest {

    /** Customer placing the order. */
    @NotBlank
    @Size(max = 64)
    private String customerId;

    @NotEmpty
    @Size(max = 50)
    @Valid
    private List<OrderLine> lines;

    @Email
    private String contactEmail;

    @JsonProperty("coupon_code")
    @Pattern(regexp = "[A-Z0-9]{6}")
    private String couponCode;

    public String getCustomerId() {
        return customerId;
    }

    public List<OrderLine> getLines() {
        return lines;
    }
}
