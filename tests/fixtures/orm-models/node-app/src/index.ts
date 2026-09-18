import express from "express";

import { publishPost } from "./services/posts";

const app = express();

app.post("/posts/:id/publish", async (req, res) => {
  await publishPost(Number(req.params.id));
  res.status(204).end();
});

app.listen(3000);
