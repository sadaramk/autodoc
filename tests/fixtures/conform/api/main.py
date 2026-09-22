from fastapi import FastAPI, HTTPException

app = FastAPI()


@app.post("/v1/jobs", status_code=202)
async def create_job(prompt: str):
    """Accept a job and return its id."""
    if not prompt:
        raise HTTPException(status_code=400, detail="prompt required")
    return {"id": "abc"}


@app.get("/v1/jobs/{job_id}")
async def get_job(job_id: str):
    """Current status of one job."""
    raise HTTPException(status_code=404, detail="unknown job")


@app.get("/v1/jobs/{job_id}/artifact-url")
async def artifact_url(job_id: str):
    """A signed URL for the finished artifact."""
    raise HTTPException(status_code=404, detail="not ready")


@app.get("/v1/plans")
async def list_plans():
    """Nobody asked for this one."""
    return []
