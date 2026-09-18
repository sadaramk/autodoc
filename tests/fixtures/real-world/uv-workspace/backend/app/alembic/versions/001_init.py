"""Initial migration."""

from alembic import op


def upgrade() -> None:
    """Backfill item owners."""
    op.execute("UPDATE item SET owner_id = 1")
