"""Account queries."""

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.sa_models import Account


def accounts_for(session: Session, owner_id: int) -> list[Account]:
    """Accounts held by an owner."""
    return list(session.scalars(select(Account).where(Account.owner_id == owner_id)))
