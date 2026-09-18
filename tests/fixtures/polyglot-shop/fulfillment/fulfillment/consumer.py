"""Kafka consumer for order events."""

import json
from typing import Callable

from confluent_kafka import Consumer

TOPIC = "order.placed"


class OrderConsumer:
    """Subscribes to `order.placed` and dispatches each order to a handler."""

    def __init__(self, brokers: str) -> None:
        self._consumer = Consumer(
            {"bootstrap.servers": brokers, "group.id": "fulfillment", "auto.offset.reset": "earliest"}
        )

    def run(self, handler: Callable[[dict], None]) -> None:
        """Poll forever, invoking `handler` for every decoded order."""
        self._consumer.subscribe([TOPIC])
        try:
            while True:
                msg = self._consumer.poll(1.0)
                if msg is None or msg.error():
                    continue
                handler(json.loads(msg.value()))
        finally:
            self._consumer.close()
