package com.acme.billing;

/** Billing's own copy of the account payload: it also expects a currency. */
public record AccountView(String id, String name, String balance, String currency) {
}
