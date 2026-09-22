import express from "express";
import { notifyShipped } from "./clients/notify";

const app = express();

app.post("/orders/:id/ship", async (req, res) => {
  await notifyShipped(req.params.id);
  res.status(202).json({ ok: true });
});

app.listen(3000);
