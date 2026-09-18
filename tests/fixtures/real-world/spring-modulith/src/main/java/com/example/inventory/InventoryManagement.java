package com.example.inventory;

import com.example.order.OrderCompleted;
import org.springframework.modulith.events.ApplicationModuleListener;
import org.springframework.stereotype.Service;

@Service
class InventoryManagement {

    @ApplicationModuleListener
    void on(OrderCompleted event) {
    }
}
