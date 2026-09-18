"""Invoice persistence."""

import os

import psycopg


def record_invoice(order_id: str) -> None:
    """Create the invoice row for an order."""
    with psycopg.connect(os.environ["DATABASE_URL"]) as conn:
        conn.execute("INSERT INTO invoices (order_id) VALUES (%s)", (order_id,))
