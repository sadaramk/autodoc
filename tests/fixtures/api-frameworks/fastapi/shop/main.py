"""Shop API."""

from fastapi import FastAPI

from shop.api.router import api_router
from shop.core.config import settings

app = FastAPI(title="Shop", openapi_url=f"{settings.API_PREFIX}/openapi.json")
app.include_router(api_router, prefix=settings.API_PREFIX)


@app.get("/healthz")
def healthz() -> dict:
    return {"ok": True}
