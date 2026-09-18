"""Data access through SQLModel."""

import uuid

from sqlmodel import Session, select

from app.models import Item, User


def list_items(session: Session) -> list[Item]:
    """Return all items."""
    return list(session.exec(select(Item)).all())


def create_item(session: Session, title: str, owner_id: uuid.UUID) -> Item:
    """Store a new item for its owner."""
    item = Item(title=title, owner_id=owner_id)
    session.add(item)
    session.commit()
    return item


def get_user_by_email(session: Session, email: str) -> User | None:
    """Look a user up by email."""
    return session.exec(select(User).where(User.email == email)).first()
