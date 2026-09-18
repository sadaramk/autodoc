from typing import Annotated

from fastapi import APIRouter, HTTPException, Path, Query, status

from shop.api.deps import CurrentUser
from shop.api.schemas import ProductCreate, ProductOut, ProductPage

router = APIRouter()


@router.get("/", response_model=ProductPage)
async def list_products(
    page: Annotated[int, Query(ge=1)] = 1,
    size: Annotated[int, Query(ge=1, le=100)] = 20,
) -> ProductPage:
    """Lists products page by page."""
    return ProductPage(items=[], total=0)


@router.get("/{product_id}")
async def get_product(product_id: Annotated[int, Path(gt=0)]) -> ProductOut:
    """Fetches one product."""
    raise HTTPException(status.HTTP_404_NOT_FOUND, "Product not found")


@router.post("/", status_code=201)
async def create_product(body: ProductCreate, user: CurrentUser) -> ProductOut:
    """Adds a product to the catalog."""
    if body.price_cents > 1_000_000:
        raise HTTPException(status_code=422, detail="Price too high")
    return ProductOut(id=1, **body.model_dump())
