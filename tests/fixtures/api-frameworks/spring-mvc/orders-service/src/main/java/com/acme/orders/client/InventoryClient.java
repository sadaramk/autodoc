package com.acme.orders.client;

import com.acme.orders.api.ReservationDto;
import org.springframework.cloud.openfeign.FeignClient;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;

@FeignClient(name = "inventory-service")
public interface InventoryClient {

    @PostMapping("/inventory/{sku}/reservations")
    ReservationDto reserve(@PathVariable("sku") String sku, @RequestBody ReserveRequest request);

    record ReserveRequest(int quantity) {
    }
}
