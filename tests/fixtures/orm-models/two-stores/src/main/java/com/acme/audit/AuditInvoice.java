package com.acme.audit;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;

/** A billed invoice, kept in the relational store. */
@Entity
@Table(name = "audit_invoices")
public class AuditInvoice {
    @Id
    private Long id;
    private String reference;
}
