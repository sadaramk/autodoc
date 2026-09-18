"""SQLite persistence for todos."""

import sqlite3


def init_db(path: str) -> None:
    """Create the todos table if it does not exist."""
    with sqlite3.connect(path) as conn:
        conn.execute("CREATE TABLE IF NOT EXISTS todos (id INTEGER PRIMARY KEY, title TEXT, done INTEGER DEFAULT 0)")


def add_todo(path: str, title: str) -> int:
    """Insert a todo and return its id."""
    with sqlite3.connect(path) as conn:
        cur = conn.execute("INSERT INTO todos (title) VALUES (?)", (title,))
        return cur.lastrowid


def list_todos(path: str) -> list[dict]:
    """Return all todos ordered by id."""
    with sqlite3.connect(path) as conn:
        rows = conn.execute("SELECT id, title, done FROM todos ORDER BY id").fetchall()
    return [{"id": r[0], "title": r[1], "done": bool(r[2])} for r in rows]
