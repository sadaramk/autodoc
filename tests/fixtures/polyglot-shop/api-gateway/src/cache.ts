import Redis from "ioredis";

const redis = new Redis(process.env.REDIS_URL ?? "redis://redis:6379");

/**
 * Read-through cache: returns the cached JSON value for `key`,
 * or computes it with `load` and stores it for `ttlSeconds`.
 */
export async function cached<T>(key: string, ttlSeconds: number, load: () => Promise<T>): Promise<T> {
  const hit = await redis.get(key);
  if (hit) {
    return JSON.parse(hit) as T;
  }
  const value = await load();
  await redis.set(key, JSON.stringify(value), "EX", ttlSeconds);
  return value;
}
