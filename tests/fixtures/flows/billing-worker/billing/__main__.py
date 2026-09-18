"""Consumes new orders and invoices them."""

import json
import os

from confluent_kafka import Consumer

from .invoices import record_invoice


def main() -> None:
    """Poll `order.created` forever."""
    consumer = Consumer({"bootstrap.servers": os.environ["KAFKA_BROKERS"], "group.id": "billing"})
    consumer.subscribe(["order.created"])
    while True:
        msg = consumer.poll(1.0)
        if msg is None or msg.error():
            continue
        order = json.loads(msg.value())
        record_invoice(order["id"])


if __name__ == "__main__":
    main()
