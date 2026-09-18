"""Item endpoints."""

from typing import Annotated, Any

from fastapi import APIRouter, Header, HTTPException, Query, status

from app.api.deps import CurrentUser
from app.api.schemas import ItemCreate, ItemPublic
from app.crud import list_items

router = APIRouter(prefix="/items", tags=["items"])


@router.get("/", response_model=list[ItemPublic])
def search_items(
    q: Annotated[str | None, Query(max_length=50)] = None,
    limit: Annotated[int, Query(ge=1, le=100)] = 20,
    x_request_id: Annotated[str | None, Header()] = None,
) -> Any:
    """Search items by title."""
    found = [i for i in list_items() if not q or q in i.title]
    return found[:limit]


@router.get("/{item_id}", response_model=ItemPublic)
def read_item(item_id: int, current_user: CurrentUser) -> Any:
    """Fetch one item by id."""
    for item in list_items():
        if item.id == item_id:
            return item
    raise HTTPException(status_code=404, detail="Item not found")


@router.post("/", response_model=ItemPublic, status_code=status.HTTP_201_CREATED)
def create_item(item_in: ItemCreate, current_user: CurrentUser) -> Any:
    """Create an item owned by the signed-in user."""
    if any(not tag.strip() for tag in item_in.tags):
        raise HTTPException(status_code=422, detail="Tags must not be blank")
    return ItemPublic(id=0, title=item_in.title, owner_email=current_user)
