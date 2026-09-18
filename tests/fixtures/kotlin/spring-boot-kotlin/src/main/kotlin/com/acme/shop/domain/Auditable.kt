package com.acme.shop.domain

import jakarta.persistence.Column
import jakarta.persistence.MappedSuperclass
import java.time.Instant

/** Timestamps every persisted row carries. */
@MappedSuperclass
abstract class Auditable(
    @Column(name = "created_at", nullable = false)
    val createdAt: Instant = Instant.EPOCH,
)
