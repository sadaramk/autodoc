"""Support tickets (in memory)."""

from dataclasses import dataclass
from enum import Enum


class TicketStatus(str, Enum):
    OPEN = "open"
    IN_PROGRESS = "in_progress"
    CLOSED = "closed"


@dataclass
class Ticket:
    title: str
    status: TicketStatus = TicketStatus.OPEN


def start(ticket: Ticket) -> None:
    """Pick up an open ticket."""
    if ticket.status == TicketStatus.OPEN:
        ticket.status = TicketStatus.IN_PROGRESS


def close(ticket: Ticket) -> None:
    """Close a ticket from any state."""
    ticket.status = TicketStatus.CLOSED
