package com.acme.ledger;

import jakarta.persistence.*;
import jakarta.validation.constraints.Positive;

@Entity
@Table(name = "ledger_entries")
public class LedgerEntry {
    @Id
    private Long id;

    @Positive
    private long amountCents;
}
