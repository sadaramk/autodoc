"""HTTP routes for todos."""

from flask import Blueprint, current_app, jsonify, request

from . import models

bp = Blueprint("todos", __name__)


@bp.get("/todos")
def list_todos():
    """Return every todo as JSON."""
    return jsonify(models.list_todos(current_app.config["DB_PATH"]))


@bp.post("/todos")
def create_todo():
    """Create a todo from the JSON body's `title`."""
    title = request.get_json(force=True)["title"]
    todo_id = models.add_todo(current_app.config["DB_PATH"], title)
    return jsonify({"id": todo_id, "title": title}), 201
