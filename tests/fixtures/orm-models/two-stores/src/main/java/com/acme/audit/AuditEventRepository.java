package com.acme.audit;

import org.springframework.data.mongodb.repository.MongoRepository;

/** Audit events live in MongoDB. */
public interface AuditEventRepository extends MongoRepository<AuditEvent, String> {}
