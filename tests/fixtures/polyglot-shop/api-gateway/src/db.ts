import { Pool } from "pg";

const pool = new Pool({ connectionString: process.env.DATABASE_URL });

/** An order row as stored in Postgres. */
export interface Order {
  id: string;
  totalCents: number;
  status: string;
}

/** Inserts a new pending order and returns it. */
export async function insertOrder(items: { sku: string; quantity: number; priceCents: number }[]): Promise<Order> {
  const total = items.reduce((sum, i) => sum + i.quantity * i.priceCents, 0);
  const { rows } = await pool.query(
    "INSERT INTO orders (total_cents, status, items) VALUES ($1, 'pending', $2) RETURNING id, total_cents AS \"totalCents\", status",
    [total, JSON.stringify(items)],
  );
  return rows[0] as Order;
}

/** Lists all active products for the catalog. */
export async function listProducts(): Promise<unknown[]> {
  const { rows } = await pool.query("SELECT sku, name, price_cents FROM products WHERE active = true");
  return rows;
}

/** Marks a pending order paid once its charge succeeded. */
export async function markOrderPaid(orderId: string): Promise<void> {
  await pool.query("UPDATE orders SET status = 'paid' WHERE id = $1 AND status = 'pending'", [orderId]);
}

/** Cancels an order that has not been paid yet. */
export async function cancelOrder(orderId: string): Promise<void> {
  await pool.query("UPDATE orders SET status = 'cancelled' WHERE id = $1 AND status = 'pending'", [orderId]);
}
