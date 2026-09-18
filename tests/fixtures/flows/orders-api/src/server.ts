import express from "express";
import { insertOrder } from "./db";
import { publishOrderCreated } from "./events";
import { startJobs } from "./jobs";

const app = express();
app.use(express.json());

app.post("/orders", async (req, res) => {
  const id = String(req.body.id);
  await insertOrder(id, Number(req.body.totalCents));
  await publishOrderCreated(id);
  res.status(201).json({ id });
});

startJobs();
app.listen(8080);
