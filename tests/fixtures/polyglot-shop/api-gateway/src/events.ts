import { Kafka } from "kafkajs";
import type { Order } from "./db";

const kafka = new Kafka({
  clientId: "api-gateway",
  brokers: (process.env.KAFKA_BROKERS ?? "kafka:9092").split(","),
});
const producer = kafka.producer();

/** Connects the shared Kafka producer; called once at startup. */
export async function connectProducer(): Promise<void> {
  await producer.connect();
}

/** Publishes `order.placed` so fulfillment can ship the order asynchronously. */
export async function publishOrderPlaced(order: Order): Promise<void> {
  await producer.send({
    topic: "order.placed",
    messages: [{ key: order.id, value: JSON.stringify(order) }],
  });
}
