package com.acme.dao;

/** Every entity can turn itself back into its domain object. */
public abstract class BaseEntity<D> {

    public abstract D toData();
}
