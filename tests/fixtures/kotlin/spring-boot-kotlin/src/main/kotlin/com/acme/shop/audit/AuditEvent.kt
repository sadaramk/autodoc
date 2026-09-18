package com.acme.shop.audit

import org.springframework.data.annotation.Id
import org.springframework.data.mongodb.core.mapping.Document
import org.springframework.data.mongodb.core.mapping.Field
import org.springframework.data.mongodb.repository.MongoRepository
import java.time.Instant

/** What happened to an order, kept for auditors. */
@Document(collection = "audit_log")
data class AuditEvent(
    @Id val id: String? = null,
    @Field("order_id") val orderId: Long = 0,
    val action: String = "",
    val at: Instant = Instant.EPOCH,
)

interface AuditRepository : MongoRepository<AuditEvent, String>
