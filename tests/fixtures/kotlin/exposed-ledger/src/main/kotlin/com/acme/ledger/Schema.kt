package com.acme.ledger

import org.jetbrains.exposed.dao.UUIDEntity
import org.jetbrains.exposed.dao.UUIDEntityClass
import org.jetbrains.exposed.dao.id.EntityID
import org.jetbrains.exposed.dao.id.IntIdTable
import org.jetbrains.exposed.dao.id.UUIDTable
import org.jetbrains.exposed.sql.Table
import org.jetbrains.exposed.sql.javatime.timestamp
import java.util.UUID

/** Who money moves between. */
object Accounts : UUIDTable("accounts") {
    val name = varchar("name", 120).uniqueIndex()
    val status = enumerationByName<AccountStatus>("status", 12).default(AccountStatus.OPEN)
    val currency = char("currency")
    val balance = decimal("balance", 14, 2).default(java.math.BigDecimal.ZERO)
    val closedAt = timestamp("closed_at").nullable()
}

/** One posting against an account. */
object Entries : IntIdTable("entries") {
    val account = reference("account_id", Accounts)
    val amount = decimal("amount", 14, 2)
    val state = enumerationByName("state", 16, EntryState::class)
    val memo = text("memo").nullable()
    val bookedAt = timestamp("booked_at").clientDefault { java.time.Instant.now() }
}

/** A free-form label, keyed by the pair it labels. */
object EntryTags : Table("entry_tags") {
    val entry = reference("entry_id", Entries)
    val tag = varchar("tag", 40)
    override val primaryKey = PrimaryKey(entry, tag)
}

enum class EntryState { DRAFT, POSTED, REVERSED }

enum class AccountStatus { OPEN, FROZEN, CLOSED }

/** The DAO view of `accounts`, used by the reconciler. */
class AccountEntity(id: EntityID<UUID>) : UUIDEntity(id) {
    companion object : UUIDEntityClass<AccountEntity>(Accounts)

    var name by Accounts.name
    var status by Accounts.status
}
