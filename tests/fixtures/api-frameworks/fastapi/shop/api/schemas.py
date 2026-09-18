"""Product schemas."""

from enum import Enum

from pydantic import BaseModel, EmailStr, Field


class Category(str, Enum):
    BOOKS = "books"
    GAMES = "games"


class ProductBase(BaseModel):
    """Shared product fields."""

    name: str = Field(..., min_length=2, max_length=120, description="Shown in listings.")
    price_cents: int = Field(ge=0, alias="priceCents")
    category: Category = Category.BOOKS


class ProductCreate(ProductBase):
    supplier_email: EmailStr
    sku: str = Field(pattern=r"^[A-Z]{3}-\d{4}$")


class ProductOut(ProductBase):
    id: int
    tags: list[str] = []


class ProductPage(BaseModel):
    items: list[ProductOut]
    total: int
