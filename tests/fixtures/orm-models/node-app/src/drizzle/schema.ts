import { integer, pgTable, serial, text, varchar } from "drizzle-orm/pg-core";

export const customers = pgTable("customers", {
  id: serial("id").primaryKey(),
  email: varchar("email", { length: 255 }).notNull().unique(),
});

export const subscriptions = pgTable("subscriptions", {
  id: serial("id").primaryKey(),
  customerId: integer("customer_id").notNull().references(() => customers.id),
  plan: text("plan"),
});

// Prettier wraps a builder chain at the default width; the same columns,
// written the way a formatter leaves them.
export const statements = pgTable("statements", {
  id: serial("id")
    .primaryKey(),
  customerId: integer("customer_id")
    .notNull()
    .references(() => customers.id),
  reference: varchar("reference", { length: 64 })
    .notNull()
    .unique(),
});
