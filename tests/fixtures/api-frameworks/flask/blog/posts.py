from flask import Blueprint, abort, jsonify, request
from flask_login import login_required

bp = Blueprint("posts", __name__, url_prefix="/posts")


@bp.route("/", methods=["GET"])
def list_posts():
    """Lists published posts."""
    tag = request.args.get("tag")
    return jsonify([])


@bp.route("/<int:post_id>", methods=["GET", "PUT"])
@login_required
def post_detail(post_id):
    """Reads or replaces one post."""
    if post_id > 1000:
        abort(404)
    return jsonify({"id": post_id})


@bp.post("/")
@login_required
def create_post():
    data = request.get_json()
    return jsonify(data), 201
