import { drizzle } from "drizzle-orm/node-postgres";

import { customers, subscriptions } from "../drizzle/schema";

const db = drizzle(process.env.DATABASE_URL!);

/** Subscribes a customer to a plan. */
export async function subscribe(customerId: number, plan: string) {
  await db.insert(subscriptions).values({ customerId, plan });
}

/** All customers. */
export async function listCustomers() {
  return db.select().from(customers);
}
