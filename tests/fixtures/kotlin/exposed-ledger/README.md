# exposed-ledger

A plain Kotlin JVM application that maps its tables with JetBrains Exposed.

Exercises: `object Accounts : UUIDTable("accounts")` / `IntIdTable` implicit generated keys; column builders
(`varchar`, `char`, `decimal(p, s)`, `text`, `timestamp`, `enumerationByName<T>`) with `.nullable()`,
`.uniqueIndex()`, `.default(…)` and `.clientDefault { }`; `reference("account_id", Accounts)` typed by the key
it points at; `override val primaryKey = PrimaryKey(entry, tag)` composite keys (unique together, not column by
column); `Entries.insert {}` / `Accounts.update {}` / `Entries.selectAll()` access; the DAO form
(`companion object : UUIDEntityClass<AccountEntity>(Accounts)`) attributed to `accounts` *through* the entity
class; an `accounts.status` lifecycle whose initial state is the column's `.default(AccountStatus.OPEN)`.
