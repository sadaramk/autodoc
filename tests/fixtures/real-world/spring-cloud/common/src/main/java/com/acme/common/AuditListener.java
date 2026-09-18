package com.acme.common;

import org.springframework.kafka.annotation.KafkaListener;
import org.springframework.stereotype.Component;

/** Shared audit consumer linked into services; not a deployable itself. */
@Component
public class AuditListener {

    @KafkaListener(topics = "audit.events")
    public void onAudit(String payload) {
    }
}
