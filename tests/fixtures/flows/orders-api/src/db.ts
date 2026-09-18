import { Pool } from "pg";

const pool = new Pool({ connectionString: process.env.DATABASE_URL });

/** Stores a new order. */
export async function insertOrder(id: string, totalCents: number): Promise<void> {
  await pool.query("INSERT INTO orders (id, total_cents) VALUES ($1, $2)", [id, totalCents]);
}

/** Marks carts idle for a day as expired. */
export async function expireCarts(): Promise<void> {
  await pool.query("UPDATE carts SET status = 'expired' WHERE updated_at < now() - interval '1 day'");
}
