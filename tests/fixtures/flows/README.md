# flows fixture

Four services exercising flows that don't start with an HTTP request, and an
event published by one language consumed by another:

- `orders-api/` (TypeScript): `POST /orders` writes `orders` and publishes Kafka `order.created`;
  a node-cron job (`0 * * * *`) expires carts.
- `billing-worker/` (Python): confluent-kafka poll loop on `order.created`, writes `invoices`.
- `inventory/` (Java, Spring): `@KafkaListener(topics = "order.created")` reserves stock and publishes
  the in-process `StockReserved` event, handled by a `@TransactionalEventListener` that writes
  `stock_audit`; a `@Scheduled(cron = …)` job reconciles stock.
- `metrics/` (Go): a `time.NewTicker` loop writing `samples`.
