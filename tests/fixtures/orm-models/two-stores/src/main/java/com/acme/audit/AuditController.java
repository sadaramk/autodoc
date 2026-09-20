package com.acme.audit;

import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RestController;

/** Records audit events. */
@RestController
public class AuditController {

    private final AuditEventRepository events;

    AuditController(AuditEventRepository events) {
        this.events = events;
    }

    /** Stores one audit event in the document store. */
    @PostMapping("/audit-events")
    public AuditEvent record(@RequestBody AuditEvent event) {
        AuditEvent saved = events.save(event);
        return saved;
    }
}
