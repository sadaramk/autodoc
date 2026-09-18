from fastapi import APIRouter, Depends

from shop.api.deps import get_current_admin

router = APIRouter(prefix="/admin", dependencies=[Depends(get_current_admin)])


@router.delete("/products/{product_id}", status_code=204)
async def delete_product(product_id: int) -> None:
    """Removes a product permanently."""
