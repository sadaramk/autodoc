from typing import Annotated

from fastapi import Depends, HTTPException


def get_current_user(token: str = "") -> str:
    if not token:
        raise HTTPException(status_code=401, detail="Not authenticated")
    return token


def get_current_admin(user: Annotated[str, Depends(get_current_user)]) -> str:
    return user


CurrentUser = Annotated[str, Depends(get_current_user)]
