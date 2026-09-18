import { Pool } from "pg";

const pool = new Pool({ connectionString: process.env.DATABASE_URL });

/** Persists a new album row. */
export async function saveAlbum(name: string): Promise<void> {
  await pool.query("INSERT INTO album (name) VALUES ($1)", [name]);
}
