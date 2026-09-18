package com.example.order;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;

/** A customer order. */
@Entity
@Table(name = "orders")
public class Order {

    @Id
    private String id;

    private String status;
}
