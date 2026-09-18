package com.acme.dao.audit;

import com.acme.dao.BaseEntity;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;

/** Written for a feature that never shipped: no subclass binds it. */
public abstract class JpaAbstractAuditDao<E extends BaseEntity<D>, D> {

    protected abstract JpaRepository<E, UUID> getRepository();

    public void purge(UUID id) {
        getRepository().deleteById(id);
    }
}
