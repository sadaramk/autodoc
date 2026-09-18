import { Router } from "express";
import { cached } from "../cache";
import { listProducts } from "../db";

export const catalogRouter = Router();

/** GET /catalog — product list, cached in Redis for 60 seconds. */
catalogRouter.get("/", async (_req, res) => {
  const products = await cached("catalog:all", 60, listProducts);
  res.json(products);
});
