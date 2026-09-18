"""Reads account balances from the accounts service."""

import httpx
from pydantic import BaseModel

ACCOUNTS = "http://accounts"


class AccountSnapshot(BaseModel):
    """What reporting stores for an account."""

    id: str
    name: str
    balance: str
    openedAt: str


def overdraft(client: httpx.Client, account_id: str) -> str:
    response = client.get(f"http://accounts/accounts/{account_id}")
    return response.json()["overdraft"]


def snapshot(client: httpx.Client, account_id: str) -> AccountSnapshot:
    response = client.get(f"http://accounts/accounts/{account_id}")
    return AccountSnapshot(**response.json())
