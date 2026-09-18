package com.acme.inventory;

import jakarta.validation.Valid;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/inventory")
public class InventoryController {

    @PostMapping("/{sku}/reservations")
    public ResponseEntity<ReservationDto> reserve(@PathVariable String sku, @Valid @RequestBody ReserveRequest request) {
        return ResponseEntity.status(HttpStatus.CREATED).body(new ReservationDto("r-1"));
    }

    @RequestMapping(value = "/{sku}", method = RequestMethod.GET)
    public StockDto stock(@PathVariable String sku) {
        return new StockDto(sku, 3);
    }

    public record ReserveRequest(@jakarta.validation.constraints.Positive int quantity) {
    }

    public record ReservationDto(String reservationId) {
    }

    public record StockDto(String sku, int available) {
    }
}
