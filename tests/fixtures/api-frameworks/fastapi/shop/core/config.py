"""Settings read from the environment."""

from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    API_PREFIX: str = "/api/v2"
    DEBUG: bool = False


settings = Settings()
