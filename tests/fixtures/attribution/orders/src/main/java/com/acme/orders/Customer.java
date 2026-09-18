package com.acme.orders;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;

/** The customer as ordering knows them: who to bill. */
@Entity
@Table(name = "customer")
public class Customer {

    @Id
    private String id;

    @Column(name = "email", nullable = false)
    private String email;

    @Column(name = "billing_address")
    private String billingAddress;
}
