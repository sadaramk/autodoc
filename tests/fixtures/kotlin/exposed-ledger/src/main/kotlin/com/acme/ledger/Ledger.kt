package com.acme.ledger

import org.jetbrains.exposed.sql.SqlExpressionBuilder.eq
import org.jetbrains.exposed.sql.insert
import org.jetbrains.exposed.sql.selectAll
import org.jetbrains.exposed.sql.update
import java.math.BigDecimal
import java.util.UUID

/** Posts entries and reads balances. */
class Ledger {

    fun post(account: UUID, amount: BigDecimal) {
        Entries.insert {
            it[Entries.account] = account
            it[Entries.amount] = amount
            it[state] = EntryState.POSTED
        }
        Accounts.update({ Accounts.id eq account }) {
            with(SqlExpressionBuilder) { it.update(balance, balance + amount) }
        }
    }

    fun entries() = Entries.selectAll().map { it[Entries.amount] }

    fun rename(account: UUID, to: String) {
        AccountEntity.findById(account)?.name = to
    }

    /** Freezing is reversible; closing is not. */
    fun freeze(account: UUID) {
        val row = AccountEntity.findById(account) ?: return
        row.status = AccountStatus.FROZEN
    }

    fun close(account: UUID) {
        val row = AccountEntity.findById(account) ?: return
        row.status = AccountStatus.CLOSED
    }
}
