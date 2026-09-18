"""Flask todo application factory."""

from flask import Flask

from .models import init_db
from .routes import bp


def create_app(db_path: str = "todos.db") -> Flask:
    """Build the Flask app with routes registered and the schema created."""
    app = Flask(__name__)
    app.config["DB_PATH"] = db_path
    init_db(db_path)
    app.register_blueprint(bp)
    return app
