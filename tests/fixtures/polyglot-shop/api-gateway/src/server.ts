import express from "express";
import { checkoutRouter } from "./routes/checkout";
import { catalogRouter } from "./routes/catalog";
import { connectProducer } from "./events";

const PORT = Number(process.env.PORT ?? 3000);

/** Builds the HTTP application with all public routes mounted. */
export function createApp(): express.Express {
  const app = express();
  app.use(express.json());
  app.use("/checkout", checkoutRouter);
  app.use("/catalog", catalogRouter);
  app.get("/healthz", (_req, res) => res.json({ ok: true }));
  return app;
}

/** Process entry point: connects Kafka, then starts listening. */
async function main(): Promise<void> {
  await connectProducer();
  createApp().listen(PORT, () => {
    console.log(`api-gateway listening on :${PORT}`);
  });
}

main();
