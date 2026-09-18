package com.acme.accounts;

/** What the accounts service publishes for an account. */
public record AccountView(String id, String name, String balance) {
}
