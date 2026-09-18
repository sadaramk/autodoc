"""Customer notifications via SendGrid."""

import os

from sendgrid import SendGridAPIClient
from sendgrid.helpers.mail import Mail


def send_shipped_email(to_email: str, order_id: str, tracking: str) -> None:
    """Email the customer that their order has shipped."""
    if not to_email:
        return
    message = Mail(
        from_email="orders@acme.shop",
        to_emails=to_email,
        subject=f"Order {order_id} has shipped",
        plain_text_content=f"Your tracking number is {tracking}.",
    )
    SendGridAPIClient(os.environ["SENDGRID_API_KEY"]).send(message)
