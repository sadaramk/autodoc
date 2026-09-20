package com.acme.audit;

import org.springframework.data.annotation.Id;
import org.springframework.data.mongodb.core.mapping.Document;

/** An audit record, kept in the document store. */
@Document(collection = "audit_events")
public class AuditEvent {
    @Id
    private String id;
    private String action;
}
