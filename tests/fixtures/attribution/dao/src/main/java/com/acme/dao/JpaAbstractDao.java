package com.acme.dao;

import jakarta.persistence.EntityManager;
import jakarta.persistence.PersistenceContext;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;

/**
 * Persistence for every DAO in the service. Which entity a call touches is
 * decided by the subclass: the repository it returns, and the type arguments it
 * binds on {@code extends}.
 */
public abstract class JpaAbstractDao<E extends BaseEntity<D>, D> {

    @PersistenceContext
    private EntityManager entityManager;

    protected abstract JpaRepository<E, UUID> getRepository();

    protected abstract E toEntity(D domain);

    public D save(D domain) {
        E entity = toEntity(domain);
        entity = getRepository().save(entity);
        return entity.toData();
    }

    public D insert(D domain) {
        E entity = toEntity(domain);
        entityManager.persist(entity);
        return entity.toData();
    }

    public D findById(UUID id) {
        return getRepository().findById(id).map(BaseEntity::toData).orElse(null);
    }

    public void removeById(UUID id) {
        JpaRepository<E, UUID> repository = getRepository();
        repository.deleteById(id);
    }
}
