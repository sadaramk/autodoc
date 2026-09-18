import Database from "better-sqlite3";
import type { NewNote, Note } from "../types";

/** SQLite-backed note persistence. */
export class NotesStore {
  private db: Database.Database;

  constructor(path: string) {
    this.db = new Database(path);
    this.db.exec(
      "CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, title TEXT, body TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP)",
    );
  }

  /** Returns all notes, newest first. */
  list(): Note[] {
    return this.db
      .prepare("SELECT id, title, body, created_at AS createdAt FROM notes ORDER BY id DESC")
      .all() as Note[];
  }

  /** Inserts a note and returns the stored row. */
  create(input: NewNote): Note {
    const info = this.db.prepare("INSERT INTO notes (title, body) VALUES (?, ?)").run(input.title, input.body);
    return this.db
      .prepare("SELECT id, title, body, created_at AS createdAt FROM notes WHERE id = ?")
      .get(info.lastInsertRowid) as Note;
  }

  /** Deletes a note by id. */
  remove(id: number): void {
    this.db.prepare("DELETE FROM notes WHERE id = ?").run(id);
  }
}
