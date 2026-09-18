"""Shipment creation and order status updates."""

import os
import uuid

import psycopg


def ship_order(order_id: str) -> str:
    """Create a shipment, mark the order shipped, and return the tracking number."""
    tracking = f"TRK-{uuid.uuid4().hex[:10].upper()}"
    with psycopg.connect(os.environ["DATABASE_URL"]) as conn:
        conn.execute(
            "UPDATE orders SET status = 'shipped', tracking_number = %s WHERE id = %s AND status = 'paid'",
            (tracking, order_id),
        )
    return tracking
