from fastapi import APIRouter

from shop.api.routes import admin, products

api_router = APIRouter()
api_router.include_router(products.router, prefix="/products", tags=["products"])
api_router.include_router(admin.router)
