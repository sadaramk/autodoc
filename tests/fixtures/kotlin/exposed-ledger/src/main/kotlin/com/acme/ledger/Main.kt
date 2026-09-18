package com.acme.ledger

import org.jetbrains.exposed.sql.Database

fun main() {
    Database.connect(System.getenv("LEDGER_DB_URL") ?: "jdbc:postgresql://db:5432/ledger")
    Ledger().entries()
}
