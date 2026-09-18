"""HTTP API serving embeddings."""

from fastapi import FastAPI

app = FastAPI()


@app.post("/predict")
def predict() -> dict:
    """Return a dummy embedding."""
    return {"embedding": [0.1, 0.2]}
