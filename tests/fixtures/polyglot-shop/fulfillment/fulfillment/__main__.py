"""Entry point for the fulfillment worker process."""

import os

from .consumer import OrderConsumer
from .notify import send_shipped_email
from .shipping import ship_order


def handle_order(order: dict) -> None:
    """Ship one order and email the customer."""
    tracking = ship_order(order["id"])
    send_shipped_email(order.get("email", ""), order["id"], tracking)


def main() -> None:
    """Consume `order.placed` forever."""
    consumer = OrderConsumer(os.environ.get("KAFKA_BROKERS", "kafka:9092"))
    consumer.run(handle_order)


if __name__ == "__main__":
    main()
