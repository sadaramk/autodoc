package com.acme.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.Entity
import jakarta.persistence.Id

@Entity
class Customer(
    @Id val id: Long = 0,
    @Column(nullable = false) val email: String = "",
)
