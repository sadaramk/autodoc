"""Voting front end."""

import json

from flask import Flask, request
from redis import Redis

app = Flask(__name__)
redis = Redis(host="redis")


@app.route("/", methods=["POST"])
def vote() -> str:
    """Record a vote for the current voter."""
    voter_id = request.cookies.get("voter_id")
    data = json.dumps({"voter_id": voter_id, "vote": request.form["vote"]})
    redis.rpush("votes", data)
    return "ok"
