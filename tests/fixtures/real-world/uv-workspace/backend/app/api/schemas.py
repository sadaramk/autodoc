"""Wire schemas for the items API."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, EmailStr, Field
from pydantic.alias_generators import to_camel


class ItemIn(BaseModel):
    """Fields a client may set on an item."""

    title: str = Field(min_length=1, max_length=255)
    description: str | None = Field(default=None, max_length=255, description="Free-text details.")


class ItemCreate(ItemIn):
    """Body of POST /api/v1/items."""

    tags: list[str] = Field(default_factory=list, max_length=5)


class ItemPublic(ItemIn):
    """An item as clients see it."""

    model_config = ConfigDict(alias_generator=to_camel, populate_by_name=True)

    id: int
    owner_email: EmailStr
    status: Literal["draft", "published"] = "draft"
