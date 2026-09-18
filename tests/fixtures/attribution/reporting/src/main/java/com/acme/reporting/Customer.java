package com.acme.reporting;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;
import java.time.Instant;

/** The customer as reporting knows them: a row in its own warehouse. */
@Entity
@Table(name = "customer")
public class Customer {

    @Id
    private String id;

    @Column(name = "cohort")
    private String cohort;

    @Column(name = "first_seen_at")
    private Instant firstSeenAt;
}
