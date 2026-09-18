"""API entry point."""

from fastapi import FastAPI

from app.api import deps
from app.api.main import api_router
from app.crud import list_items

app = FastAPI()
app.include_router(api_router, prefix=deps.API_V1_STR)


@app.get("/items")
def items() -> list:
    """List stored items."""
    return list_items()


@app.get("/health")
def health() -> dict:
    """Liveness probe for the load balancer."""
    return {"ok": True}
