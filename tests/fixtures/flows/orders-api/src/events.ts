import { Kafka } from "kafkajs";

const kafka = new Kafka({ clientId: "orders-api", brokers: [process.env.KAFKA_BROKERS ?? "kafka:9092"] });
const producer = kafka.producer();

/** Announces a new order to billing and inventory. */
export async function publishOrderCreated(id: string): Promise<void> {
  await producer.send({
    topic: "order.created",
    messages: [{ key: id, value: JSON.stringify({ id }) }],
  });
}
