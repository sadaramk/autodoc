package com.acme.ledger;

import jakarta.persistence.EntityManager;
import java.util.List;

public class LedgerStore {
    private final EntityManager em;

    public LedgerStore(EntityManager em) {
        this.em = em;
    }

    public void record(LedgerEntry ledgerEntry) {
        em.persist(ledgerEntry);
    }

    public List<LedgerEntry> recent() {
        return em.createQuery("select e from LedgerEntry e order by e.id desc", LedgerEntry.class).getResultList();
    }
}
