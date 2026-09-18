import { Hono } from "hono";
import { zValidator } from "@hono/zod-validator";
import { z } from "zod";

const bookSchema = z.object({
  title: z.string().min(1),
  year: z.number().int().gte(1450),
});

const books = new Hono();

books.get("/:isbn", (c) => {
  const isbn = c.req.param("isbn");
  const fields = c.req.query("fields");
  return c.json({ error: "book not found" }, 404);
});

books.post("/", zValidator("json", bookSchema), (c) => {
  return c.json({ id: 1, title: "x" }, 201);
});

const app = new Hono();
app.route("/books", books);

export default app;
