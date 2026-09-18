package com.acme.billing;

/** Row shape billing expects from the account list. */
public record AccountSummary(String id, String name, String tier) {
}
