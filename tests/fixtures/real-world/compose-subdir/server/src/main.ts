import express from "express";
import Redis from "ioredis";
import { mlUrl } from "./config";
import { saveAlbum } from "./db";

const redis = new Redis(process.env.REDIS_URL ?? "redis://redis:6379");

/** Starts the photo server. */
export function main(): void {
  const app = express();
  app.post("/albums", async (_req, res) => {
    await saveAlbum("holiday");
    res.json({ ml: mlUrl(), cached: await redis.ping() });
  });
  app.listen(2283);
}

main();
