"""Request dependencies shared by the routes."""

from typing import Annotated

from fastapi import Depends, Header, HTTPException, status

API_V1_STR = "/api/v1"


def get_current_user(authorization: Annotated[str | None, Header()] = None) -> str:
    """Resolves the signed-in user's email from the bearer token."""
    if not authorization:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Not authenticated")
    return authorization.removeprefix("Bearer ")


CurrentUser = Annotated[str, Depends(get_current_user)]
